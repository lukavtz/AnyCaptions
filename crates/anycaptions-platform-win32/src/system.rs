//! Native operating-system actions shared by the GPUI windows.

use anycaptions_core::update::RELEASES_PAGE_URL;
use std::sync::{
    LazyLock,
    mpsc::{self, Receiver, Sender},
};
use std::time::Duration;
use windows::Win32::Foundation::{COLORREF, E_FAIL, HWND};
use windows::Win32::System::Diagnostics::Debug::Beep;
use windows::Win32::UI::Controls::Dialogs::{
    CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW, CommDlgExtendedError,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SW_SHOWNORMAL, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW,
};
use windows::core::{BOOL, Error, HSTRING, PCWSTR, Result, w};

pub const PROJECT_PAGE_URL: &str = "https://github.com/EduardoViniciusJ";
pub const DICTATION_CUE_DURATION: Duration = Duration::from_millis(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserPage {
    Project,
    Releases,
}

impl BrowserPage {
    pub const fn url(self) -> &'static str {
        match self {
            Self::Project => PROJECT_PAGE_URL,
            Self::Releases => RELEASES_PAGE_URL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationCue {
    Start,
    Stop,
}

impl DictationCue {
    pub const fn frequency_hz(self) -> u32 {
        match self {
            Self::Start => 880,
            Self::Stop => 440,
        }
    }
}

static CUE_QUEUE: LazyLock<Option<Sender<DictationCue>>> = LazyLock::new(|| {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("dictation-cue".into())
        .spawn(move || cue_worker(receiver, play_native_cue))
        .ok()
        .map(|_| sender)
});

fn cue_sender() -> Option<&'static Sender<DictationCue>> {
    CUE_QUEUE.as_ref()
}

fn cue_worker(receiver: Receiver<DictationCue>, mut play: impl FnMut(DictationCue)) {
    while let Ok(cue) = receiver.recv() {
        play(cue);
    }
}

fn play_native_cue(cue: DictationCue) {
    // SAFETY: frequency and duration are fixed valid Win32 Beep inputs.
    let _ = unsafe {
        Beep(
            cue.frequency_hz(),
            DICTATION_CUE_DURATION.as_millis() as u32,
        )
    };
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemActions;

impl SystemActions {
    /// Opens the native color dialog. Colors use `0xRRGGBB`; `None` means cancel.
    pub fn choose_color(owner: Option<HWND>, initial: u32) -> Result<Option<u32>> {
        let mut custom = [COLORREF(0x00FF_FFFF); 16];
        let mut dialog = CHOOSECOLORW {
            lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
            hwndOwner: owner.unwrap_or_default(),
            rgbResult: to_colorref(initial),
            lpCustColors: custom.as_mut_ptr(),
            Flags: CC_RGBINIT | CC_FULLOPEN,
            ..Default::default()
        };
        // SAFETY: `dialog` and its custom-color buffer outlive the modal call.
        if unsafe { ChooseColorW(&mut dialog) }.as_bool() {
            return Ok(Some(from_colorref(dialog.rgbResult)));
        }
        // SAFETY: this only reads the thread's common-dialog error state.
        let error = unsafe { CommDlgExtendedError() };
        if error.0 == 0 {
            Ok(None)
        } else {
            Err(Error::new(
                E_FAIL,
                format!("ChooseColorW failed with common-dialog error {}", error.0),
            ))
        }
    }

    /// Opens one of two fixed pages; no UI-controlled URL reaches ShellExecuteW.
    pub fn open_browser(page: BrowserPage) -> Result<()> {
        let url = HSTRING::from(page.url());
        // SAFETY: all strings outlive the call; ShellExecuteW only reads them.
        let result = unsafe {
            ShellExecuteW(
                None,
                w!("open"),
                PCWSTR(url.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        let code = result.0 as isize;
        if code > 32 {
            Ok(())
        } else {
            Err(Error::new(
                E_FAIL,
                format!(
                    "Default browser could not open {} (ShellExecuteW {code})",
                    page.url()
                ),
            ))
        }
    }

    /// Queues a short cue through the single FIFO audio worker without blocking GPUI.
    /// Audio failure is non-fatal because the visual state remains available.
    pub fn beep(cue: DictationCue) {
        if let Some(sender) = cue_sender() {
            let _ = sender.send(cue);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReducedMotion(bool);

impl ReducedMotion {
    pub const fn from_enabled(client_area_animation: bool) -> Self {
        Self(!client_area_animation)
    }

    pub fn query() -> Result<Self> {
        let mut animation = BOOL(0);
        // SAFETY: `animation` is a valid writable BOOL and no update is requested.
        unsafe {
            SystemParametersInfoW(
                SPI_GETCLIENTAREAANIMATION,
                0,
                Some((&mut animation as *mut BOOL).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        }?;
        Ok(Self::from_enabled(animation.as_bool()))
    }

    pub const fn is_reduced(self) -> bool {
        self.0
    }

    pub fn animation_duration(self, duration: Duration) -> Duration {
        if self.0 { Duration::ZERO } else { duration }
    }
}

pub(crate) const fn to_colorref(rgb: u32) -> COLORREF {
    COLORREF(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF))
}

pub(crate) const fn from_colorref(color: COLORREF) -> u32 {
    ((color.0 & 0xFF) << 16) | (color.0 & 0xFF00) | ((color.0 >> 16) & 0xFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colorref_conversion_preserves_rgb_channels() {
        for rgb in [0x000000, 0xFFFFFF, 0xA1B2C3, 0xFF0000, 0x00FF00, 0x0000FF] {
            assert_eq!(from_colorref(to_colorref(rgb)), rgb);
        }
        assert_eq!(to_colorref(0xA1B2C3).0, 0xC3B2A1);
    }

    #[test]
    fn browser_pages_are_compile_time_constants() {
        assert_eq!(BrowserPage::Project.url(), PROJECT_PAGE_URL);
        assert_eq!(BrowserPage::Releases.url(), RELEASES_PAGE_URL);
        assert!(BrowserPage::Project.url().starts_with("https://"));
        assert!(BrowserPage::Releases.url().starts_with("https://"));
    }

    #[test]
    fn dictation_cues_match_the_existing_tone_contract() {
        assert_eq!(DictationCue::Start.frequency_hz(), 880);
        assert_eq!(DictationCue::Stop.frequency_hz(), 440);
        assert_eq!(DICTATION_CUE_DURATION, Duration::from_millis(120));
    }

    #[test]
    fn dictation_cues_use_one_fifo_worker() {
        let (sender, receiver) = mpsc::channel();
        for cue in [DictationCue::Start, DictationCue::Stop, DictationCue::Start] {
            sender.send(cue).unwrap();
        }
        drop(sender);
        let mut played = Vec::new();
        cue_worker(receiver, |cue| played.push(cue));
        assert_eq!(
            played,
            [DictationCue::Start, DictationCue::Stop, DictationCue::Start]
        );
    }

    #[test]
    fn reduced_motion_removes_only_animation_time() {
        let reduced = ReducedMotion::from_enabled(false);
        let normal = ReducedMotion::from_enabled(true);
        let fade = Duration::from_millis(120);
        assert_eq!(reduced.animation_duration(fade), Duration::ZERO);
        assert_eq!(normal.animation_duration(fade), fade);
    }
}
