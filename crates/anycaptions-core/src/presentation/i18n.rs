#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Locale {
    #[default]
    PtBr,
    En,
    Es,
    ZhCn,
    Fr,
    Ru,
}

impl Locale {
    pub const ALL: &'static [Self] = &[
        Self::PtBr,
        Self::En,
        Self::Es,
        Self::ZhCn,
        Self::Fr,
        Self::Ru,
    ];

    pub fn from_tag(tag: &str) -> Option<Self> {
        UI_LANGUAGES
            .iter()
            .find(|option| option.tag.eq_ignore_ascii_case(tag))
            .map(|option| option.locale)
    }

    pub const fn tag(self) -> &'static str {
        UI_LANGUAGES[self as usize].tag
    }

    pub const fn country(self) -> &'static str {
        UI_LANGUAGES[self as usize].country
    }

    pub const fn name(self) -> &'static str {
        UI_LANGUAGES[self as usize].name
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiLanguageOption {
    pub locale: Locale,
    pub tag: &'static str,
    pub country: &'static str,
    pub name: &'static str,
}

pub const UI_LANGUAGES: &[UiLanguageOption; 6] = &[
    UiLanguageOption {
        locale: Locale::PtBr,
        tag: "pt-br",
        country: "br",
        name: "Português (Brasil)",
    },
    UiLanguageOption {
        locale: Locale::En,
        tag: "en",
        country: "us",
        name: "English",
    },
    UiLanguageOption {
        locale: Locale::Es,
        tag: "es",
        country: "es",
        name: "Español",
    },
    UiLanguageOption {
        locale: Locale::ZhCn,
        tag: "zh-cn",
        country: "cn",
        name: "中文（简体）",
    },
    UiLanguageOption {
        locale: Locale::Fr,
        tag: "fr",
        country: "fr",
        name: "Français",
    },
    UiLanguageOption {
        locale: Locale::Ru,
        tag: "ru",
        country: "ru",
        name: "Русский",
    },
];

macro_rules! message_catalog {
    ($( $variant:ident => [$pt_br:expr, $en:expr, $es:expr, $zh_cn:expr, $fr:expr, $ru:expr] ),+ $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum MessageKey {
            $( $variant, )+
        }

        impl MessageKey {
            pub const ALL: &'static [Self] = &[$( Self::$variant, )+];
        }

        const MESSAGES: &[&[&str; 6]] = &[
            $( &[$pt_br, $en, $es, $zh_cn, $fr, $ru], )+
        ];

        pub fn translate(locale: Locale, key: MessageKey) -> Option<&'static str> {
            MESSAGES.get(key as usize)?.get(locale as usize).copied()
        }
    };
}

message_catalog! {
    CaptionWindow => ["Janela de legendas", "Caption window", "Ventana de subtítulos", "字幕窗口", "Fenêtre de sous-titres", "Окно субтитров"],
    CaptionControls => ["Controles de legendas", "Caption controls", "Controles de subtítulos", "字幕控制", "Contrôles des sous-titres", "Управление субтитрами"],
    DragMove => ["Arraste para mover a janela", "Drag to move the window", "Arrastra para mover la ventana", "拖动以移动窗口", "Faites glisser pour déplacer la fenêtre", "Перетащите, чтобы переместить окно"],
    DragWidth => ["Arraste para ajustar a largura", "Drag to adjust the width", "Arrastra para ajustar el ancho", "拖动以调整宽度", "Faites glisser pour ajuster la largeur", "Перетащите, чтобы изменить ширину"],
    DragHeight => ["Arraste para ajustar a altura", "Drag to adjust the height", "Arrastra para ajustar la altura", "拖动以调整高度", "Faites glisser pour ajuster la hauteur", "Перетащите, чтобы изменить высоту"],
    Start => ["Iniciar", "Start", "Iniciar", "开始", "Démarrer", "Старт"],
    Stop => ["Parar", "Stop", "Detener", "停止", "Arrêter", "Стоп"],
    CaptureHint => ["A captura precisa de uma saída de áudio e de modelos disponíveis", "Capture needs an audio output and available models", "La captura necesita una salida de audio y modelos disponibles", "捕获需要音频输出和可用的模型", "La capture nécessite une sortie audio et des modèles disponibles", "Для захвата нужны аудиовыход и доступные модели"],
    CaptionLanguage => ["Idioma das legendas", "Caption language", "Idioma de los subtítulos", "字幕语言", "Langue des sous-titres", "Язык субтитров"],
    AutoMode => ["Automático", "Automatic", "Automático", "自动", "Automatique", "Автоматически"],
    TranscribeMode => ["Transcrever", "Transcribe", "Transcribir", "转写", "Transcrire", "Транскрибировать"],
    AutoModeHint => ["Detecta o idioma e transcreve no idioma falado", "Detects the language and transcribes the spoken language", "Detecta el idioma y transcribe el idioma hablado", "检测语言并按所说语言转写", "Détecte la langue et transcrit la langue parlée", "Определяет язык и транскрибирует устную речь"],
    TranslateTo => ["Traduzir para", "Translate to", "Traducir a", "翻译为", "Traduire en", "Перевести на"],
    MoreLanguagesRiva => ["Outros idiomas (chinês, japonês, espanhol…): baixe o modelo Riva nas configurações", "More languages (Chinese, Japanese, Spanish…): download the Riva model in the settings", "Más idiomas (chino, japonés, español…): descarga el modelo Riva en la configuración", "更多语言（中文、日语、西班牙语……）：请在设置中下载 Riva 模型", "Autres langues (chinois, japonais, espagnol…) : téléchargez le modèle Riva dans les réglages", "Другие языки (китайский, японский, испанский…): скачайте модель Riva в настройках"],
    Settings => ["Configurações", "Settings", "Configuración", "设置", "Paramètres", "Настройки"],
    SettingsAria => ["Configurações de legendas", "Caption settings", "Configuración de subtítulos", "字幕设置", "Paramètres des sous-titres", "Настройки субтитров"],
    LockWindow => ["Bloquear janela", "Lock window", "Bloquear ventana", "锁定窗口", "Verrouiller la fenêtre", "Заблокировать окно"],
    LockHint => ["O bloqueio precisa de atalho ou bandeja disponíveis", "Locking needs an available shortcut or tray", "El bloqueo necesita un atajo o la bandeja disponibles", "锁定需要可用的快捷键或托盘", "Le verrouillage nécessite un raccourci ou la barre d'état", "Для блокировки нужны горячая клавиша или трей"],
    CloseToTray => ["Recolher à bandeja", "Minimize to tray", "Minimizar a la bandeja", "最小化到托盘", "Réduire dans la barre d'état", "Свернуть в трей"],
    CaptionPlaceholder => ["Suas legendas aparecem aqui", "Your captions appear here", "Tus subtítulos aparecen aquí", "您的字幕将显示在这里", "Vos sous-titres apparaissent ici", "Здесь появляются ваши субтитры"],
    Preview => ["Prévia", "Preview", "Vista previa", "预览", "Aperçu", "Предпросмотр"],
    PreviewAria => ["Prévia da legenda", "Caption preview", "Vista previa del subtítulo", "字幕预览", "Aperçu du sous-titre", "Предпросмотр субтитров"],
    PreviewFinal => ["Texto ", "Text ", "El texto ", "文字", "Le texte ", "Текст "],
    PreviewPartial => ["fica claro", "stays clear", "queda claro", "保持清晰", "reste clair", "остаётся чётким"],
    FontSize => ["Tamanho", "Size", "Tamaño", "大小", "Taille", "Размер"],
    ConfirmedText => ["Texto confirmado", "Confirmed text", "Texto confirmado", "已确认文本", "Texte confirmé", "Подтверждённый текст"],
    RecentWords => ["Palavras recentes", "Recent words", "Palabras recientes", "最近词语", "Mots récents", "Недавние слова"],
    CaptionBackground => ["Fundo escuro", "Dark background", "Fondo oscuro", "深色背景", "Fond sombre", "Тёмный фон"],
    PickConfirmedColor => ["Escolher cor do texto confirmado", "Pick the confirmed text color", "Elegir el color del texto confirmado", "选择已确认文本的颜色", "Choisir la couleur du texte confirmé", "Выбрать цвет подтверждённого текста"],
    ConfirmedColorHex => ["Cor hexadecimal do texto confirmado", "Confirmed text hex color", "Color hexadecimal del texto confirmado", "已确认文本的十六进制颜色", "Couleur hexadécimale du texte confirmé", "HEX-цвет подтверждённого текста"],
    PickRecentColor => ["Escolher cor das palavras recentes", "Pick the recent words color", "Elegir el color de las palabras recientes", "选择最近词语的颜色", "Choisir la couleur des mots récents", "Выбрать цвет недавних слов"],
    RecentColorHex => ["Cor hexadecimal das palavras recentes", "Recent words hex color", "Color hexadecimal de las palabras recientes", "最近词语的十六进制颜色", "Couleur hexadécimale des mots récents", "HEX-цвет недавних слов"],
    AudioOutput => ["Saída de áudio", "Audio output", "Salida de audio", "音频输出", "Sortie audio", "Аудиовыход"],
    UiLanguage => ["Idioma da interface", "Interface language", "Idioma de la interfaz", "界面语言", "Langue de l'interface", "Язык интерфейса"],
    SourceLanguage => ["Idioma falado (origem)", "Spoken language (source)", "Idioma hablado (origen)", "语音语言（来源）", "Langue parlée (source)", "Язык речи (источник)"],
    TranslationEngine => ["Modelo de tradução", "Translation model", "Modelo de traducción", "翻译模型", "Modèle de traduction", "Модель перевода"],
    EngineOpus => ["OPUS-MT — rápido (CPU), inglês ↔ português", "OPUS-MT — fast (CPU), English ↔ Portuguese", "OPUS-MT — rápido (CPU), inglés ↔ portugués", "OPUS-MT——快速（CPU），英语 ↔ 葡萄牙语", "OPUS-MT — rapide (CPU), anglais ↔ portugais", "OPUS-MT — быстрая (CPU), английский ↔ португальский"],
    EngineRiva => ["Riva — 37 idiomas (GPU)", "Riva — 37 languages (GPU)", "Riva — 37 idiomas (GPU)", "Riva——37 种语言（GPU）", "Riva — 37 langues (GPU)", "Riva — 37 языков (GPU)"],
    ModelDownloadMissing => ["O modelo deste mecanismo ainda não foi baixado.", "This engine's model has not been downloaded yet.", "El modelo de este motor aún no se ha descargado.", "该引擎的模型尚未下载。", "Le modèle de ce moteur n'a pas encore été téléchargé.", "Модель этого движка ещё не загружена."],
    ModelDownloadAction => ["Baixar ({size} GB)", "Download ({size} GB)", "Descargar ({size} GB)", "下载（{size} GB）", "Télécharger ({size} Go)", "Скачать ({size} ГБ)"],
    ModelDownloadActionMb => ["Baixar ({size} MB)", "Download ({size} MB)", "Descargar ({size} MB)", "下载（{size} MB）", "Télécharger ({size} Mo)", "Скачать ({size} МБ)"],
    ModelDownloadCancel => ["Cancelar download", "Cancel download", "Cancelar descarga", "取消下载", "Annuler le téléchargement", "Отменить загрузку"],
    ModelDownloadProgress => ["Baixando… {percent}%", "Downloading… {percent}%", "Descargando… {percent}%", "下载中… {percent}%", "Téléchargement… {percent} %", "Загрузка… {percent}%"],
    ModelDownloadRetry => ["Tentar de novo", "Try again", "Intentar de nuevo", "重试", "Réessayer", "Повторить"],
    ModelDownloadFailedNetwork => ["Falha na conexão. Verifique a internet e tente de novo.", "Connection failed. Check your internet and try again.", "Falló la conexión. Revisa tu internet e inténtalo de nuevo.", "连接失败。请检查网络后重试。", "Échec de la connexion. Vérifiez votre internet et réessayez.", "Сбой соединения. Проверьте интернет и повторите попытку."],
    ModelDownloadFailedDisk => ["Não foi possível gravar o arquivo. Verifique o espaço em disco.", "Could not write the file. Check your disk space.", "No se pudo guardar el archivo. Revisa el espacio en disco.", "无法写入文件。请检查磁盘空间。", "Impossible d'écrire le fichier. Vérifiez l'espace disque.", "Не удалось записать файл. Проверьте место на диске."],
    ModelDownloadFailedCorrupted => ["O arquivo baixado não passou na verificação. Tente de novo.", "The downloaded file failed verification. Try again.", "El archivo descargado no pasó la verificación. Inténtalo de nuevo.", "下载的文件未通过校验。请重试。", "Le fichier téléchargé a échoué à la vérification. Réessayez.", "Загруженный файл не прошёл проверку. Повторите попытку."],
    SpeakerLabels => ["Identificação de falantes", "Speaker identification", "Identificación de hablantes", "说话人识别", "Identification des locuteurs", "Определение говорящих"],
    SpeakerLabelsEnable => ["Identificar quem está falando", "Show who is speaking", "Mostrar quién está hablando", "显示是谁在说话", "Indiquer qui parle", "Показывать, кто говорит"],
    SpeakerLabelsHint => ["Cada voz recebe um rótulo genérico (Pessoa 1, Pessoa 2…) a partir do som; o app não reconhece identidades nem nomes. Usa um modelo adicional e mais GPU/CPU; vozes sobrepostas podem ser atribuídas a uma só pessoa.", "Each voice gets a generic label (Person 1, Person 2…) from the sound alone; the app does not recognize identities or names. It uses an extra model and more GPU/CPU; overlapping voices may be attributed to one person.", "Cada voz recibe una etiqueta genérica (Persona 1, Persona 2…) a partir del sonido; la app no reconoce identidades ni nombres. Usa un modelo adicional y más GPU/CPU; las voces superpuestas pueden atribuirse a una sola persona.", "每个声音仅根据音频获得一个通用标签（说话人 1、说话人 2……）；应用不会识别身份或姓名。需要额外的模型和更多 GPU/CPU 资源；重叠的声音可能被归为同一个人。", "Chaque voix reçoit une étiquette générique (Personne 1, Personne 2…) d’après le son seul ; l’application ne reconnaît ni identités ni noms. Elle utilise un modèle supplémentaire et plus de GPU/CPU ; des voix qui se chevauchent peuvent être attribuées à une seule personne.", "Каждый голос получает условную метку (Участник 1, Участник 2…) только по звуку; приложение не распознаёт личности и имена. Нужна дополнительная модель и больше ресурсов GPU/CPU; перекрывающиеся голоса могут быть отнесены к одному человеку."],
    SpeakerLabelsModelMissing => ["O modelo de identificação de falantes ainda não foi baixado.", "The speaker identification model has not been downloaded yet.", "El modelo de identificación de hablantes aún no se ha descargado.", "说话人识别模型尚未下载。", "Le modèle d’identification des locuteurs n’a pas encore été téléchargé.", "Модель определения говорящих ещё не загружена."],
    SpeakerLabelsUnavailable => ["Identificação de falantes indisponível; as legendas seguem sem rótulos.", "Speaker identification unavailable; captions continue without labels.", "Identificación de hablantes no disponible; los subtítulos continúan sin etiquetas.", "说话人识别不可用；字幕将继续显示但没有标签。", "Identification des locuteurs indisponible ; les sous-titres continuent sans étiquettes.", "Определение говорящих недоступно; субтитры продолжаются без меток."],
    SpeakerLabel => ["Pessoa {n}", "Person {n}", "Persona {n}", "说话人 {n}", "Personne {n}", "Участник {n}"],
    CaptionFont => ["Fonte", "Font", "Fuente", "字体", "Police", "Шрифт"],
    DevelopedBy => ["desenvolvido por", "developed by", "desarrollado por", "开发者", "développé par", "разработано"],
    Updates => ["Atualizações", "Updates", "Actualizaciones", "更新", "Mises à jour", "Обновления"],
    AppVersion => ["Versão", "Version", "Versión", "版本", "Version", "Версия"],
    UpdateCheckOnStart => ["Verificar nova versão ao abrir o app", "Check for a new version when the app opens", "Buscar una versión nueva al abrir la app", "打开应用时检查新版本", "Vérifier une nouvelle version à l’ouverture", "Проверять новую версию при запуске"],
    UpdateCheckNow => ["Verificar agora", "Check now", "Buscar ahora", "立即检查", "Vérifier maintenant", "Проверить сейчас"],
    UpdateChecking => ["Verificando…", "Checking…", "Buscando…", "正在检查…", "Vérification…", "Проверка…"],
    UpdateAvailable => ["Nova versão {version} disponível.", "New version {version} available.", "Nueva versión {version} disponible.", "有新版本 {version} 可用。", "Nouvelle version {version} disponible.", "Доступна новая версия {version}."],
    UpdateLatest => ["Você já tem a versão mais recente ({version}).", "You already have the latest version ({version}).", "Ya tienes la versión más reciente ({version}).", "您已是最新版本（{version}）。", "Vous avez déjà la dernière version ({version}).", "У вас уже последняя версия ({version})."],
    UpdateFailed => ["Não foi possível verificar agora. Tente de novo mais tarde.", "Could not check right now. Try again later.", "No se pudo comprobar ahora. Inténtalo más tarde.", "现在无法检查，请稍后再试。", "Vérification impossible pour le moment. Réessayez plus tard.", "Сейчас проверить не удалось. Попробуйте позже."],
    UpdateDownload => ["Baixar", "Download", "Descargar", "下载", "Télécharger", "Скачать"],
    UpdateInstall => ["Atualizar", "Update", "Actualizar", "更新", "Mettre à jour", "Обновить"],
    UpdateDownloading => ["Baixando atualização… {percent}%", "Downloading update… {percent}%", "Descargando actualización… {percent}%", "正在下载更新… {percent}%", "Téléchargement de la mise à jour… {percent} %", "Загрузка обновления… {percent}%"],
    UpdateInstalling => ["Instalando atualização…", "Installing update…", "Instalando actualización…", "正在安装更新…", "Installation de la mise à jour…", "Установка обновления…"],
    UpdateInstallFailed => ["Não foi possível instalar a atualização. Nada mudou.", "Could not install the update. Nothing changed.", "No se pudo instalar la actualización. No cambió nada.", "无法安装更新。未作任何更改。", "Impossible d'installer la mise à jour. Rien n'a changé.", "Не удалось установить обновление. Ничего не изменилось."],
    UpdateHint => ["Consulta a página de versões do projeto no GitHub; a atualização só é instalada quando você clica.", "Looks up the project's releases page on GitHub; updates install only when you click.", "Consulta la página de versiones del proyecto en GitHub; la actualización solo se instala cuando haces clic.", "查询 GitHub 上的项目版本页面；只有在你点击后才会安装更新。", "Consulte la page des versions du projet sur GitHub ; la mise à jour ne s'installe que lorsque vous cliquez.", "Проверяет страницу версий проекта на GitHub; обновление устанавливается только по вашему нажатию."],
    NoOutputs => ["Nenhuma saída disponível", "No output available", "Ninguna salida disponible", "没有可用的输出", "Aucune sortie disponible", "Нет доступных выходов"],
    SelectOutput => ["Selecione uma saída", "Select an output", "Selecciona una salida", "选择一个输出", "Sélectionnez une sortie", "Выберите выход"],
    CheckingOutputs => ["Verificando saídas…", "Checking outputs…", "Comprobando salidas…", "正在检查输出…", "Vérification des sorties…", "Проверка выходов…"],
    SavedOutputUnavailable => ["Saída salva indisponível", "Saved output unavailable", "Salida guardada no disponible", "已保存的输出不可用", "Sortie enregistrée indisponible", "Сохранённый выход недоступен"],
    NoActiveOutput => ["Nenhuma saída de reprodução ativa.", "No active playback output.", "Ninguna salida de reproducción activa.", "没有活动的播放输出。", "Aucune sortie de lecture active.", "Нет активного выхода воспроизведения."],
    SavedOutputDisconnected => ["A saída salva foi desconectada. Escolha outra saída.", "The saved output was disconnected. Pick another output.", "La salida guardada se desconectó. Elige otra salida.", "已保存的输出已断开。请选择其他输出。", "La sortie enregistrée a été déconnectée. Choisissez-en une autre.", "Сохранённый выход отключён. Выберите другой."],
    LockShortcut => ["Atalho de bloqueio", "Lock shortcut", "Atajo de bloqueo", "锁定快捷键", "Raccourci de verrouillage", "Клавиша блокировки"],
    ShortcutUnavailableTray => ["Atalho indisponível; use a bandeja para recuperar os controles.", "Shortcut unavailable; use the tray to recover the controls.", "Atajo no disponible; usa la bandeja para recuperar los controles.", "快捷键不可用；请使用托盘恢复控制。", "Raccourci indisponible ; utilisez la barre d'état pour récupérer les contrôles.", "Клавиша недоступна; используйте трей, чтобы вернуть управление."],
    RestoreDefault => ["Restaurar padrão", "Restore default", "Restaurar predeterminado", "恢复默认", "Rétablir par défaut", "Сбросить по умолчанию"],
    Save => ["Salvar", "Save", "Guardar", "保存", "Enregistrer", "Сохранить"],
    ErrCapture => ["A captura de áudio ainda está indisponível.", "Audio capture is still unavailable.", "La captura de audio aún no está disponible.", "音频捕获暂不可用。", "La capture audio est encore indisponible.", "Захват звука пока недоступен."],
    ErrTranscription => ["O modelo de transcrição está indisponível.", "The transcription model is unavailable.", "El modelo de transcripción no está disponible.", "转写模型不可用。", "Le modèle de transcription est indisponible.", "Модель транскрипции недоступна."],
    ErrTranslation => ["O modelo de tradução está indisponível.", "The translation model is unavailable.", "El modelo de traducción no está disponible.", "翻译模型不可用。", "Le modèle de traduction est indisponible.", "Модель перевода недоступна."],
    ErrFontSize => ["Escolha um tamanho de texto entre 16 e 80 px.", "Choose a text size between 16 and 80 px.", "Elige un tamaño de texto entre 16 y 80 px.", "请选择 16 到 80 px 之间的文字大小。", "Choisissez une taille de texte entre 16 et 80 px.", "Выберите размер текста от 16 до 80 px."],
    ErrRecovery => ["O bloqueio precisa de um atalho ou da bandeja para recuperar os controles.", "Locking needs a shortcut or the tray to recover the controls.", "El bloqueo necesita un atajo o la bandeja para recuperar los controles.", "锁定需要快捷键或托盘来恢复控制。", "Le verrouillage nécessite un raccourci ou la barre d'état pour récupérer les contrôles.", "Для блокировки нужны горячая клавиша или трей, чтобы вернуть управление."],
    ErrShortcutFormat => ["Use um atalho no formato Ctrl+Alt+L (letra de A a Z).", "Use a shortcut like Ctrl+Alt+L (letter A to Z).", "Usa un atajo con el formato Ctrl+Alt+L (letra de A a Z).", "请使用 Ctrl+Alt+L 格式的快捷键（字母 A 到 Z）。", "Utilisez un raccourci au format Ctrl+Alt+L (lettre de A à Z).", "Используйте сочетание вида Ctrl+Alt+L (буква от A до Z)."],
    ErrShortcutTaken => ["Esse atalho está em uso. Escolha outra letra; a bandeja continua disponível.", "That shortcut is in use. Pick another letter; the tray stays available.", "Ese atajo está en uso. Elige otra letra; la bandeja sigue disponible.", "该快捷键已被占用。请选择其他字母；托盘仍然可用。", "Ce raccourci est déjà utilisé. Choisissez une autre lettre ; la barre d'état reste disponible.", "Это сочетание занято. Выберите другую букву; трей остаётся доступным."],
    ErrOutputGone => ["Essa saída de áudio não está disponível. Atualize a lista e escolha outra.", "That audio output is not available. Refresh the list and pick another.", "Esa salida de audio no está disponible. Actualiza la lista y elige otra.", "该音频输出不可用。请刷新列表并选择其他输出。", "Cette sortie audio n'est pas disponible. Actualisez la liste et choisissez-en une autre.", "Этот аудиовыход недоступен. Обновите список и выберите другой."],
    ErrOutputInvalid => ["Selecione uma saída de áudio válida.", "Select a valid audio output.", "Selecciona una salida de audio válida.", "请选择有效的音频输出。", "Sélectionnez une sortie audio valide.", "Выберите корректный аудиовыход."],
    ErrGeneric => ["Não foi possível concluir a ação. Tente novamente.", "The action could not be completed. Try again.", "No se pudo completar la acción. Inténtalo de nuevo.", "操作未能完成。请重试。", "L'action n'a pas pu être effectuée. Réessayez.", "Не удалось выполнить действие. Попробуйте ещё раз."],
    ErrConnect => ["Não foi possível conectar à aplicação. Feche e abra novamente.", "Could not connect to the application. Close it and open it again.", "No se pudo conectar con la aplicación. Ciérrala y ábrela de nuevo.", "无法连接到应用程序。请关闭后重新打开。", "Impossible de se connecter à l'application. Fermez-la et rouvrez-la.", "Не удалось подключиться к приложению. Закройте и откройте его снова."],
    Dictation => ["Ditado por voz", "Voice dictation", "Dictado por voz", "语音听写", "Dictée vocale", "Голосовой ввод"],
    DictationEnable => ["Ativar ditado", "Enable dictation", "Activar dictado", "启用听写", "Activer la dictée", "Включить голосовой ввод"],
    DictationMic => ["Microfone", "Microphone", "Micrófono", "麦克风", "Microphone", "Микрофон"],
    DictationDefaultMic => ["Padrão do sistema", "System default", "Predeterminado del sistema", "系统默认", "Par défaut du système", "Системный по умолчанию"],
    DictationShortcut => ["Atalho do ditado", "Dictation shortcut", "Atajo del dictado", "听写快捷键", "Raccourci de dictée", "Сочетание клавиш"],
    DictationActivation => ["Modo de ativação", "Activation mode", "Modo de activación", "激活方式", "Mode d'activation", "Режим активации"],
    DictationHold => ["Segurar para falar", "Hold to talk", "Mantener para hablar", "按住说话", "Maintenir pour parler", "Удерживать для записи"],
    DictationToggle => ["Alternar (toque liga e desliga)", "Toggle (tap to start and stop)", "Alternar (un toque inicia y otro termina)", "切换（按一下开始，再按结束）", "Basculer (appui pour démarrer et arrêter)", "Переключение (нажатие начинает и завершает)"],
    DictationSound => ["Sinal sonoro ao gravar", "Sound cue while recording", "Señal sonora al grabar", "录音提示音", "Signal sonore pendant l'enregistrement", "Звуковой сигнал при записи"],
    DictationLanguage => ["Idioma do ditado", "Dictation language", "Idioma del dictado", "听写语言", "Langue de dictée", "Язык диктовки"],
    DictationTranslate => ["Traduzir ditado para", "Translate dictation to", "Traducir dictado a", "将听写翻译为", "Traduire la dictée en", "Переводить диктовку на"],
    DictationNoTranslate => ["Não traduzir", "Don't translate", "No traducir", "不翻译", "Ne pas traduire", "Не переводить"],
    DictationShowText => ["Mostrar texto reconhecido", "Show recognized text", "Mostrar texto reconocido", "显示识别文本", "Afficher le texte reconnu", "Показывать распознанный текст"],
    DictationLimitDuration => ["Limitar duração", "Limit duration", "Limitar duración", "限制时长", "Limiter la durée", "Ограничить длительность"],
    DictationMaxDuration => ["Duração máxima (s)", "Maximum duration (s)", "Duración máxima (s)", "最长时长（秒）", "Durée maximale (s)", "Максимальная длительность (с)"],
    DictationModelMissing => ["Modelo de transcrição indisponível; o ditado não funcionará.", "Transcription model unavailable; dictation will not work.", "Modelo de transcripción no disponible; el dictado no funcionará.", "转写模型不可用，听写无法工作。", "Modèle de transcription indisponible ; la dictée ne fonctionnera pas.", "Модель распознавания недоступна; голосовой ввод не будет работать."],
    DictationListening => ["Ouvindo… solte para enviar", "Listening… release to send", "Escuchando… suelta para enviar", "正在聆听……松开发送", "Écoute… relâchez pour envoyer", "Слушаю… отпустите, чтобы отправить"],
    DictationListeningToggle => ["Gravando — atalho finaliza, Esc descarta", "Recording — shortcut finishes, Esc discards", "Grabando — el atajo termina, Esc descarta", "录音中 — 快捷键结束，Esc 取消", "Enregistrement — le raccourci termine, Échap annule", "Запись — сочетание завершает, Esc отменяет"],
    DictationFinalizing => ["Finalizando…", "Finalizing…", "Finalizando…", "正在完成……", "Finalisation…", "Завершение…"],
    ErrDictation => ["O ditado falhou. Tente novamente.", "Dictation failed. Try again.", "El dictado falló. Inténtalo de nuevo.", "听写失败，请重试。", "La dictée a échoué. Réessayez.", "Голосовой ввод не удался. Попробуйте ещё раз."],
    ErrDictationMic => ["O ditado não conseguiu usar o microfone.", "Dictation could not use the microphone.", "El dictado no pudo usar el micrófono.", "听写无法使用麦克风。", "La dictée n'a pas pu utiliser le microphone.", "Не удалось использовать микрофон."],
    ErrDictationModel => ["O modelo de transcrição do ditado está indisponível.", "The dictation transcription model is unavailable.", "El modelo de transcripción del dictado no está disponible.", "听写的转写模型不可用。", "Le modèle de transcription de la dictée est indisponible.", "Модель распознавания для голосового ввода недоступна."],
    ErrDictationPaste => ["Não foi possível colar. O texto está na área de transferência (Ctrl+V).", "Could not paste. The text is on the clipboard (Ctrl+V).", "No se pudo pegar. El texto está en el portapapeles (Ctrl+V).", "无法粘贴。文本已在剪贴板中（Ctrl+V）。", "Collage impossible. Le texte est dans le presse-papiers (Ctrl+V).", "Не удалось вставить. Текст в буфере обмена (Ctrl+V)."],
    ErrDictationShortcutFormat => ["Use F6–F24 ou Ctrl/Alt + tecla (ex.: Ctrl+Space).", "Use F6–F24 or Ctrl/Alt + key (e.g. Ctrl+Space).", "Usa F6–F24 o Ctrl/Alt + tecla (p. ej. Ctrl+Space).", "使用 F6–F24 或 Ctrl/Alt + 按键（如 Ctrl+Space）。", "Utilisez F6–F24 ou Ctrl/Alt + touche (ex. Ctrl+Space).", "Используйте F6–F24 или Ctrl/Alt + клавишу (например, Ctrl+Space)."],
    ErrDictationShortcutConflict => ["Esse atalho já está em uso pelo bloqueio da janela.", "That shortcut is already used by the window lock.", "Ese atajo ya lo usa el bloqueo de la ventana.", "该快捷键已被窗口锁定功能占用。", "Ce raccourci est déjà utilisé par le verrouillage de la fenêtre.", "Это сочетание уже занято блокировкой окна."],
    ErrDictationDuration => ["A duração máxima deve ficar entre 5 e 300 segundos.", "The maximum duration must be between 5 and 300 seconds.", "La duración máxima debe estar entre 5 y 300 segundos.", "最长时长必须在 5 到 300 秒之间。", "La durée maximale doit être comprise entre 5 et 300 secondes.", "Максимальная длительность должна быть от 5 до 300 секунд."],
    ErrDictationLanguage => ["Idioma do ditado inválido. Escolha uma opção da lista.", "Invalid dictation language. Pick an option from the list.", "Idioma de dictado no válido. Elige una opción de la lista.", "听写语言无效。请从列表中选择。", "Langue de dictée non valide. Choisissez une option de la liste.", "Недопустимый язык диктовки. Выберите вариант из списка."],
    ErrDictationTranslate => ["A tradução do ditado falhou. O texto falado ficou na área de transferência (Ctrl+V para colar).", "Dictation translation failed. The spoken text was left on the clipboard (Ctrl+V to paste).", "La traducción del dictado falló. El texto hablado quedó en el portapapeles (Ctrl+V para pegar).", "听写翻译失败。口述文本已留在剪贴板中（Ctrl+V 粘贴）。", "La traduction de la dictée a échoué. Le texte dicté reste dans le presse-papiers (Ctrl+V pour coller).", "Перевод диктовки не удался. Произнесённый текст остался в буфере обмена (Ctrl+V для вставки)."],
}
