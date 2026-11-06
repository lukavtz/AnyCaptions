# Model alternatives for local Windows captions

**Research date:** 2026-11-06
**Scope:** Handy, OpenWhisper, OpenWhispr, OpenAI Whisper tiny/base/small/medium/turbo, and the existing NVIDIA NeMo/Riva/OPUS-MT pipeline. Sources are official project documentation, repositories, model cards, and first-party runtime measurements. No local inference benchmark was run for this document.

## Decision

1. **Keep `nemotron-3.5-asr-streaming-0.6b` as the default multilingual live-caption ASR.** None of Whisper tiny, base, small, medium, or turbo is a native streaming replacement. OpenAI says Whisper is not real-time out of the box; Handy's `transcribe.cpp` Whisper contract explicitly excludes real-time streaming, VAD, speaker diarization, and word timestamps. [[OpenAI model card](https://github.com/openai/whisper/blob/main/model-card.md)] [[transcribe.cpp Whisper](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/whisper.md)]
2. **Evaluate `nvidia/nemotron-speech-streaming-en-0.6b` as the first optional English-only profile; integrate it only after local A/B validation.** It is not a multilingual replacement. It retains a 600M-parameter cache-aware FastConformer-RNNT design and is supported by the same official NeMo-Speech.cpp runtime family as the current model. On Handy's same-method Q8_0 FLEURS English table it has lower WER and higher throughput than the multilingual model, but that does not prove better end-to-end captions on AnyCaptions hardware. [[English model card](https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b)] [[NeMo-Speech.cpp models](https://github.com/NVIDIA/NeMo-Speech.cpp#models-and-applications)] [[Handy English measurements](https://models.handy.computer/languages/en/)]
3. **Supplement, rather than replace, with Whisper where its behavior fits:**
   - **Small multilingual Q8_0:** the best Whisper size/accuracy compromise for an offline spike: 270 MB and 244M parameters, with competitive English/Portuguese WER but worse French WER. Do not integrate it into live captions or add a backend without an approved offline use case.
   - **Turbo multilingual Q8_0:** a future 886 MB offline accuracy/retranscription option only. It is not a live-caption or translation replacement; OpenAI explicitly says Turbo is not trained for translation.
   - **Medium:** an offline alternative, not a default addition. On Handy's table it is slightly smaller and faster than Turbo but has higher WER in the three compared languages.
   - **Tiny/base:** do not replace the current ASR. Their small artifacts buy substantially higher WER in the published comparable tables. Use only if a separate draft/offline use case explicitly accepts that trade-off.
4. **Do not adopt the Handy application as the captions engine.** Handy records microphone segments and transcribes afterward; its FAQ says real-time/live captions are not yet supported and system audio is not currently transcribed. Its reusable `transcribe.cpp` runtime is the relevant technology, not the Handy UI/workflow. [[Handy FAQ](https://handy.computer/docs/faq.html)]
5. **Treat OpenWhisper as an orchestration/reference implementation, not proof that Local Whisper is streaming-native.** Its Whisper/Parakeet preview re-decodes overlapping windows and final ASR remains separate. Nemotron Streaming has a cache-aware incremental path; Parakeet Unified has a separate buffered-streaming path. [[OpenWhisper preview design](https://github.com/Knuckles92/OpenWhisper/blob/main/docs/preview-optimizations.md)] [[OpenWhisper speech models](https://github.com/Knuckles92/OpenWhisper/blob/main/docs/speech-models.md)] [[Parakeet Unified model card](https://huggingface.co/nvidia/parakeet-unified-en-0.6b)]
6. **Keep Riva Translate 4B, OPUS-MT, and NeMo diarization.** Whisper's speech-to-English task is not general bidirectional text NMT, Turbo cannot supply that task, and standard Whisper does not replace the current eight-speaker streaming diarizer. [[OpenAI Whisper tasks](https://github.com/openai/whisper#command-line-usage)] [[Riva Translate 4B card](https://huggingface.co/nvidia/Riva-Translate-4B-Instruct-v2)] [[Nemotron 3 Diarization card](https://huggingface.co/nvidia/Nemotron-3-Diarization)]

## 1. Evaluation rules and current baseline

Parameter count, artifact size, runtime memory, model capability, and application behavior are different axes:

- **Parameters** describe the checkpoint, not download bytes or total process memory.
- **Artifact size** depends on format, precision, quantization, tokenizer/metadata, and distributor. GGML, GGUF, CTranslate2, `.nemo`, and PyTorch sizes must not be interchanged.
- **Runtime fit** includes the inference engine, backend, resident weights plus compute buffers, concurrent NMT/diarization models, and Windows driver/toolkit constraints.
- **Feature fit** includes native incremental decoding, partial/final semantics, language coverage, timestamps, endpointing, speaker labels, and translation direction. A faster-than-real-time batch decode does not by itself provide low-latency streaming captions.

### Existing pipeline to preserve

| Role | Current baseline and decision |
| --- | --- |
| Streaming ASR | `nemotron-3.5-asr-streaming-0.6b`: 600M parameters, cache-aware FastConformer-RNNT, punctuation/capitalization, configurable 80–1120 ms chunks. NVIDIA lists 40 language-locales, but only 32 are transcription-ready or broad-coverage; eight are adaptation-ready and require fine-tuning. Keep as multilingual default. [[Model card](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b)] |
| Local ASR artifact | `packaging/artifacts.lock.json` pins `nemotron-3.5-asr-streaming-0.6b.q8_0.gguf` at **742,090,464 bytes**, SHA-256 `3fc991d3badad7277c11030a7519832cddaf2057aafed6d4b25147e953a070b1`. NVIDIA's file view displays the same artifact as 742 MB. This exact Q8 file must not be confused with separately converted 751 MB Q8_0 artifacts in Handy's catalog. [[NVIDIA artifact](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/tree/main)] [[Handy conversion](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/nemotron-3.5-asr-streaming-0.6b.md)] |
| Text translation | Riva Translate 4B Instruct v2 translates text between English and 36 non-English languages and supports sentence- and document-level translation. AnyCaptions downloads a separate 2,759,302,368-byte Q4_K_M GGUF on demand. Keep. [[Model card](https://huggingface.co/nvidia/Riva-Translate-4B-Instruct-v2)] |
| Additional translation | OPUS-MT provides downloadable Marian-NMT translation models by language pair. AnyCaptions ships INT8 CTranslate2 English↔Portuguese models on CPU. Whisper ASR does not subsume these text-translation paths. Keep. [[OPUS-MT](https://github.com/Helsinki-NLP/Opus-MT)] |
| Diarization | `nvidia/Nemotron-3-Diarization` is a separate streaming/offline Sortformer for up to eight speakers, with a lowest recommended input-buffer latency of 0.32 s. AnyCaptions downloads its 107,012,128-byte Q8_0 GGUF on demand and aligns ASR words to its 10 ms speaker-probability frames. Keep. [[Model card](https://huggingface.co/nvidia/Nemotron-3-Diarization)] |

## 2. Project identities and supported models

### 2.1 Handy

[Handy](https://github.com/cjpais/Handy) is a free, open-source, offline desktop dictation application for Windows, macOS, and Linux. Current `src-tauri/Cargo.toml` routes every model through `transcribe-cpp` (GGUF/ggml); Windows x86_64 enables dynamic CPU backends and Vulkan, while Windows ARM64 is CPU-only. The runtime also publishes Rust bindings. [[Handy backend declaration](https://github.com/cjpais/Handy/blob/main/src-tauri/Cargo.toml)] [[transcribe.cpp bindings](https://github.com/handy-computer/transcribe.cpp#bindings)]

The **Handy application's** published model list is below. These are Handy-reported approximate downloads, not OpenAI parameter counts or universal artifact sizes. [[Handy model documentation](https://handy.computer/docs/models.html)]

| Family | Handy-exposed models | Published size and role |
| --- | --- | --- |
| Whisper | Small, Medium, Large v3, Large v3 Turbo | ~487 MB, ~492 MB, ~1.1 GB, ~1.6 GB; broad multilingual ASR; Small/Medium/Large also offer speech-to-English translation, Turbo does not |
| Parakeet | TDT 0.6B v3, v2 | ~478 MB for 25 European languages; ~473 MB English-only; offline in the reusable runtime's standard v2/v3 path |
| Cohere Transcribe | 2B, 14 languages | ~1.8 GB quantized GGUF; GPU-oriented |
| Moonshine | V2 Tiny, Small, Medium; Base | ~31 MB, ~100 MB, ~192 MB, ~58 MB; English-only offline models |
| Canary | 180M Flash, 1B v2 | ~146 MB for four languages; ~692 MB for 25 languages; transcription and selected translation paths |
| SenseVoice | SenseVoice Small | ~160 MB; English, Chinese, Japanese, Korean, Cantonese |
| GigaAM | GigaAM v3 | ~225 MB; Russian |
| Breeze ASR | Breeze ASR 25 | ~1.1 GB; Taiwanese Mandarin/code-switching Whisper variant |

Handy says Whisper and Cohere use Metal on macOS and Vulkan on Windows/Linux; its other listed model families currently run on CPU. The application supports custom Whisper-compatible `.bin` files experimentally. [[Handy hardware notes](https://handy.computer/docs/models.html#hardware-considerations)]

**Application versus runtime:** Handy's UI list is not the full `transcribe.cpp` catalog. The reusable runtime additionally documents Nemotron streaming, Parakeet Unified, Moonshine Streaming, streaming Sortformer, and other families. Evaluate those as runtime candidates without claiming the Handy app exposes them. [[transcribe.cpp catalog](https://github.com/handy-computer/transcribe.cpp#supported-models)]

### 2.2 OpenWhisper

[OpenWhisper (`Knuckles92/OpenWhisper`)](https://github.com/Knuckles92/OpenWhisper) is an MIT-licensed Python/PyQt desktop application for dictation, files, and meetings on Windows, macOS, and Linux. It is a different project from `OpenWhispr/openwhispr`; section 2.3 covers the latter so neither spelling is confused with the other. Local Whisper uses `faster-whisper`/CTranslate2, while custom Whisper folders require `model.bin`, `config.json`, and `tokenizer.json`. [[OpenWhisper dependencies](https://github.com/Knuckles92/OpenWhisper/blob/main/pyproject.toml)] [[Custom model contract](https://openwhisper.fiorilabs.tech/docs/choosing-a-model/#add-a-custom-whisper-model)]

| Backend | Published models and Windows relevance |
| --- | --- |
| Local Whisper | Standard Whisper sizes, Turbo, and Distil-Whisper; CPU on all platforms, NVIDIA CUDA on Windows/Linux x86_64; dictation preview, uploads, meetings |
| Parakeet | TDT 0.6B v3 and Orukeet 0.6B; Windows x64 CPU/NVIDIA GPU; preview, uploads, meeting chunks; standard v3 is offline at model-runtime level |
| Parakeet Unified | English Unified EN 0.6B; Windows x64 CPU or Vulkan GPU (NVIDIA/AMD/Intel); native preview and offline final decode; published Q8 weights are 731 MB |
| Nemotron Streaming | Nemotron 3.5 ASR Streaming 0.6B; Windows x64 CPU/NVIDIA GPU; native streaming preview, uploads, meetings |
| Moonshine Streaming | Small and Medium, English-only; Windows x64 CPU; native preview, uploads, meetings |
| Qwen3-ASR | 0.6B and 1.7B; Windows x64 CPU/NVIDIA GPU; dictation and uploads |
| Parakeet MLX | TDT 0.6B v3, Apple Silicon/macOS 14+ only; not a Windows candidate |
| API / remote computer | OpenAI/Groq or an OpenAI-compatible local/remote server, or another paired OpenWhisper host; network boundary depends on configuration |

Sources: [[OpenWhisper model table](https://github.com/Knuckles92/OpenWhisper#speech-models)] [[OpenWhisper model details](https://github.com/Knuckles92/OpenWhisper/blob/main/docs/speech-models.md)] [[OpenWhisper Windows install](https://openwhisper.fiorilabs.tech/docs/installation/)]

For Local Whisper on Windows, OpenWhisper requires an NVIDIA driver providing CUDA 12 (525+) for GPU acceleration, but not the CUDA Toolkit; CPU remains available. Parakeet and Nemotron have separate downloadable runtimes, with CPU alternatives. New Windows x64 installations default to Parakeet, not Whisper. [[GPU requirements](https://openwhisper.fiorilabs.tech/docs/choosing-a-model/#download-models-and-runtimes)]

OpenWhisper's meeting speaker identification is separate from ASR: on-device WeSpeaker labels are the default and an optional consented cloud pass uses OpenAI diarization. This is not evidence of parity with the existing NeMo streaming diarizer. [[Speaker-label documentation](https://github.com/Knuckles92/OpenWhisper/blob/main/docs/speech-models.md#api-transcription-servers)]

### 2.3 OpenWhispr, a separate project

[OpenWhispr (`OpenWhispr/openwhispr`)](https://github.com/OpenWhispr/openwhispr) is an Electron/React desktop app built with Node.js, `whisper.cpp`, and sherpa-onnx. Its [local-model guide](https://docs.openwhispr.com/guides/local-models) lists these download sizes and roles:

| Family | OpenWhispr-published size and capability |
| --- | --- |
| Orukeet | ~672 MB; multilingual Parakeet fine-tune and the default local model |
| Whisper | tiny ~75 MB, base ~142 MB, small ~466 MB, medium ~1.5 GB, large ~3 GB, turbo ~1.6 GB through `whisper.cpp` |
| Parakeet | TDT 0.6B v3 ~680 MB and 25 languages; Unified EN 0.6B ~631 MB and English-only |
| Nemotron | English Streaming 0.6B ~632 MB; 3.5 ASR Streaming 0.6B ~650 MB with partial text over a persistent stream |
| Cohere Transcribe | ~2.7 GB on disk, 14 languages, no automatic language detection |

On Windows, OpenWhispr documents downloadable CUDA runtimes for NVIDIA and Vulkan runtimes for AMD/Intel, with CPU fallback. Its Whisper sizes refer to the app's standard GGML artifacts, not the Q8_0 GGUF artifacts in section 3. [[OpenWhispr Whisper setup](https://github.com/OpenWhispr/openwhispr/blob/main/LOCAL_WHISPER_SETUP.md)]

OpenWhispr's Nemotron path is the relevant comparison: it uses a streaming model and live partials, while its Whisper family provides the wider size range. This supports keeping native streaming ASR separate from optional offline Whisper backends. OpenWhispr's guide says the multilingual Nemotron covers 15 languages; NVIDIA's publisher card lists 40 locales in three tiers. The publisher's current model card governs this document's language claim. [[OpenWhispr local models](https://docs.openwhispr.com/guides/local-models)] [[NVIDIA multilingual model card](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b#supported-languages)]

## 3. Exact Whisper identities and published artifact sizes

### 3.1 OpenAI-published model table

The parameter counts and approximate VRAM below come directly from OpenAI's README. They are **published rounded parameter counts**, not values inferred from files. Standard GGML disk figures come from the official `whisper.cpp` model catalog; Q8_0 GGUF figures come from Handy's official `transcribe.cpp` catalog. [[OpenAI model table](https://github.com/openai/whisper#available-models-and-languages)] [[whisper.cpp model catalog](https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md)] [[transcribe.cpp Whisper catalog](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/whisper.md)]

| Size | OpenAI parameters | English-only name | Multilingual name | OpenAI approximate VRAM | OpenAI relative speed | Standard GGML disk | `transcribe.cpp` Q8_0 GGUF |
| --- | ---: | --- | --- | ---: | ---: | ---: | ---: |
| tiny | 39 M | `tiny.en` | `tiny` | ~1 GB | ~10× | 75 MiB | 46 MB |
| base | 74 M | `base.en` | `base` | ~1 GB | ~7× | 142 MiB | 85 MB |
| small | 244 M | `small.en` | `small` | ~2 GB | ~4× | 466 MiB | 270 MB |
| medium | 769 M | `medium.en` | `medium` | ~5 GB | ~2× | 1.5 GiB | 832 MB |
| large | 1550 M | none | `large` (currently aliased to large-v3) | ~10 GB | 1× | 2.9 GiB | 1.67 GB for large-v3 |
| turbo | **809 M** | none | `turbo` / `large-v3-turbo` | ~6 GB | ~8× | 1.5 GiB | 886 MB |

OpenAI measured relative speed for English transcription on an A100 against large; these are not Windows latency measurements. The sizes describe different artifacts and units; do not compare a Q8 GGUF directly with an original `.pt`, `.nemo`, CTranslate2, or unquantized GGML download. OpenAI's table publishes VRAM rather than original `.pt` byte counts, so no original-PyTorch download sizes are asserted here.

### 3.2 Language and task distinctions

- Tiny, base, small, and medium each have English-only `.en` and multilingual variants at the same published parameter tier. Large and Turbo have no English-only variant. [[OpenAI model table](https://github.com/openai/whisper#available-models-and-languages)]
- OpenAI says `.en` tends to help most at tiny/base, with less difference at small/medium. English-only models cannot transcribe other languages or translate. Handy's runtime documents 99 languages for the earlier multilingual checkpoints and 100 for large-v3/Turbo, which adds Cantonese. [[OpenAI language note](https://github.com/openai/whisper#available-models-and-languages)] [[transcribe.cpp variant guide](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/whisper.md#choosing-a-variant)]
- Multilingual tiny/base/small/medium/large support transcription and speech translation **to English**. This is not arbitrary source/target text translation. [[OpenAI task documentation](https://github.com/openai/whisper#command-line-usage)]
- **Turbo is multilingual for transcription but is not trained for translation.** Asking for `translate` still returns the original language. It reduces large-v3 decoder layers from 32 to four. [[OpenAI Turbo warning](https://github.com/openai/whisper#command-line-usage)] [[Turbo model card](https://huggingface.co/openai/whisper-large-v3-turbo)]

### 3.3 Streaming and diarization boundaries

OpenAI's reference `transcribe()` reads the whole input and uses a sliding 30-second window; its model card says Whisper cannot do real-time transcription out of the box. `transcribe.cpp` similarly exposes 30-second chunked long-form decoding, segment timestamps, and translation on multilingual non-Turbo checkpoints, but explicitly no real-time streaming, VAD, speaker diarization, or exposed word timestamps. [[OpenAI README](https://github.com/openai/whisper#python-usage)] [[OpenAI model card](https://github.com/openai/whisper/blob/main/model-card.md)] [[transcribe.cpp capabilities](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/whisper.md#capabilities)]

A buffered application can still show near-real-time previews. OpenWhisper's documented window preview uses three-second windows with 0.75-second overlap and separately produces final ASR. That is useful orchestration, but it is not equivalent to the current model's cache-aware incremental stream. [[OpenWhisper preview measurements and design](https://github.com/Knuckles92/OpenWhisper/blob/main/docs/preview-optimizations.md)]

`whisper.cpp` lists a special `small.en-tdrz` variant with local speaker-turn support through tinydiarize. That special English model is not standard Whisper tiny/base/small/medium/turbo and is not evidence for replacing NeMo streaming diarization. [[whisper.cpp model catalog](https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md#available-models)]

## 4. Same-method published comparison

Handy's first-party tables compare Q8_0 artifacts on FLEURS and report speed on the **same AMD Ryzen 4750U**, using Vulkan for GPU and the listed CPU path. WER is lower-is-better; speed is higher-is-better. Sizes below reproduce the site's rounded `GB` display and do not replace the exact artifact sizes above. [[English](https://models.handy.computer/languages/en/)] [[Portuguese](https://models.handy.computer/languages/pt/)] [[French](https://models.handy.computer/languages/fr/)]

| Model | English WER % | Portuguese WER % | French WER % | AMD GPU / CPU speed | Displayed size |
| --- | ---: | ---: | ---: | ---: | ---: |
| **Current Nemotron 3.5 multilingual streaming 0.6B** | 7.90 | 8.52 | 10.78 | 17.1× / 10.6× | 0.70 GB |
| **Nemotron English streaming 0.6B** | **6.43** | n/a | n/a | **27.1× / 13.6×** | 0.68 GB |
| Parakeet Unified EN 0.6B | 3.99 | n/a | n/a | 25.3× / 14.0× | 0.68 GB |
| Parakeet TDT 0.6B v3 (offline) | 4.83 | 4.96 | 5.30 | 25.1× / 13.7× | 0.69 GB |
| Moonshine Streaming Tiny (English) | 18.18 | n/a | n/a | 59.1× / 56.3× | 0.05 GB |
| Moonshine Streaming Small (English) | 8.55 | n/a | n/a | 21.8× / 13.5× | 0.18 GB |
| Moonshine Streaming Medium (English) | 7.87 | n/a | n/a | 14.3× / 9.7× | 0.28 GB |
| Whisper tiny | 13.84 | 24.07 | 44.19 | 43.1× / 39.8× | 0.04 GB |
| Whisper base | 9.88 | 13.91 | 27.91 | 29.0× / 19.3× | 0.08 GB |
| Whisper small | 6.51 | 7.65 | 13.30 | 11.7× / 5.9× | 0.25 GB |
| Whisper medium | 4.64 | 5.07 | 8.07 | 4.1× / 1.9× | 0.77 GB |
| Whisper large-v3 turbo | 4.38 | 4.17 | 5.51 | 3.1× / 1.4× | 0.83 GB |
| Whisper large-v3 | 4.03 | 3.88 | 5.39 | 2.0× / 1.0× | 1.55 GB |

For English-only Whisper variants on the same English table: `tiny.en` 10.72%, `base.en` 7.60%, `small.en` 6.14%, and `medium.en` 4.88%. They remain offline models. [[Handy English measurements](https://models.handy.computer/languages/en/)]

### What the table supports

- **English Nemotron is the strongest low-risk optional experiment:** lower English WER than the current multilingual model and higher reported throughput at the same 600M class. It says nothing about Portuguese/French and does not remove NMT or diarization.
- **Whisper Small is a footprint/optional-offline candidate, not a universal quality win:** it beats the current multilingual model's English and Portuguese WER in this table but is worse on French and lacks native streaming.
- **Turbo and large-v3 are accuracy-oriented offline candidates:** lower WER in all three languages comes with much lower reported throughput and larger artifacts; neither supplies native streaming.
- **Moonshine Streaming provides smaller native-streaming English alternatives**, and OpenWhisper exposes Small/Medium. English Nemotron has lower English WER than all three in this table. `transcribe.cpp` also limits each Moonshine Streaming decode/stream to 81.9 seconds, requiring deliberate stream rollover for indefinite captions. [[Moonshine Streaming contract](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/moonshine-streaming.md)]
- **Parakeet Unified is another English streaming candidate, not a smaller-parameter model:** it remains 0.6B/618M in Handy's catalog and does not cover other languages. Standard Parakeet v3 is offline-only despite its strong throughput. [[Parakeet runtime contract](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/parakeet.md)]

### What the table does not support

These numbers do not establish end-to-end live-caption latency, partial-text stability, long-session behavior, accuracy on AnyCaptions audio, or superiority on the production Windows GPU. Handy measured its own converted artifacts, not necessarily AnyCaptions' locked NVIDIA revisions. The public tables do not specify a full microphone-to-partial-to-final streaming methodology. They justify local A/B tests, not a real-time superiority claim.

## 5. Windows and acceleration constraints

| Path | Documented constraint relevant to AnyCaptions |
| --- | --- |
| Handy app | Windows supported; x86_64 uses Vulkan/dynamic CPU backends, ARM64 is CPU-only. App itself has no live captions or system-audio transcription. [[Handy source](https://github.com/cjpais/Handy/blob/main/src-tauri/Cargo.toml)] [[FAQ](https://handy.computer/docs/faq.html)] |
| `transcribe.cpp` | Native C/C++ GGUF/ggml runtime with CPU, Vulkan, CUDA, and Metal paths, Windows build documentation, and official Rust bindings. Vulkan is the documented Windows GPU route in Handy. [[Runtime README](https://github.com/handy-computer/transcribe.cpp)] |
| OpenWhisper Local Whisper | Windows 10/11 64-bit; CPU or NVIDIA CUDA 12 driver 525+; no CUDA Toolkit required by the packaged component. No AMD/Intel Vulkan path is documented for Local Whisper. [[OpenWhisper GPU guide](https://openwhisper.fiorilabs.tech/docs/choosing-a-model/)] |
| OpenWhisper other local engines | Parakeet Unified supports Vulkan across GPU vendors; Parakeet/Nemotron support Windows x64 CPU and NVIDIA GPU. Each model may require a separate runtime download. [[OpenWhisper models](https://github.com/Knuckles92/OpenWhisper#speech-models)] |
| NeMo-Speech.cpp | Official native Windows x86_64/aarch64 installation path, with CPU, CUDA, and Vulkan options. CUDA needs toolkit/driver for source builds; Vulkan needs the LunarG SDK and vendor driver. This is distinct from Python NeMo and Riva NIM. [[Windows install](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/install.md#windows)] |
| Python NeMo model integration | The multilingual Nemotron card lists Linux/Linux for Tegra as supported operating systems and uses Linux installation commands. Do not infer native Windows support for that Python route; the native Windows evidence belongs to NeMo-Speech.cpp. [[Model card software integration](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b#software-integration)] |
| Riva ASR/NIM | Riva documents offline and streaming ASR plus streaming diarization, but its Whisper Large v3 NIM profile is offline-only. Current ASR NIM documents Linux or Windows 11 through WSL2, Compute Capability 8.0+, and at least 16 GB VRAM; Nemotron ASR Streaming and Whisper Large v3 are marked not WSL2-supported. [[Riva ASR](https://docs.nvidia.com/deeplearning/riva/user-guide/docs/asr/asr-overview.html)] [[ASR NIM matrix](https://docs.nvidia.com/nim/speech/latest/reference/support-matrix/asr.html)] |
| Riva NMT/NIM | Current NMT NIM documentation lists Linux, Compute Capability 8.0+, at least 16 GB VRAM, and Riva Translate **1.6B** with no WSL support. This does not describe every local use of the separate Riva Translate **4B** model; NeMo-Speech.cpp explicitly supports that 4B model on its native runtime. [[NMT NIM matrix](https://docs.nvidia.com/nim/speech/latest/reference/support-matrix/nmt.html)] [[NeMo-Speech.cpp model support](https://github.com/NVIDIA/NeMo-Speech.cpp#models-and-applications)] |

## 6. Recommendation by role

| Candidate | Recommendation | Reason |
| --- | --- | --- |
| Current multilingual Nemotron 0.6B | **Retain default** | Native streaming, broadest production-relevant language role in the current pipeline, existing pinned artifact; no Whisper model offers a like-for-like streaming replacement |
| English Nemotron 0.6B | **Add as optional English profile after A/B validation** | Same model/runtime family and native streaming; better same-table English WER/throughput; not smaller in parameters and English-only |
| Parakeet Unified EN 0.6B | **Secondary optional English experiment** | Buffered streaming plus offline final decode and Vulkan; same broad size class, English-only, different runtime and converted artifact |
| Moonshine Streaming Small/Medium | **Optional resource-constrained English experiment only** | Smaller native-streaming artifacts and OpenWhisper precedent, but higher English WER than English Nemotron and an 81.9-second stream/decode ceiling in `transcribe.cpp` |
| Whisper Small multilingual | **Offline spike only after a use case is approved; no live-caption integration** | 244M parameters and 270 MB Q8_0; broad languages; competitive English/Portuguese WER but worse French and no native stream |
| Whisper Turbo multilingual | **Possible future offline/retranscription option; no live-caption integration** | 809M published parameters and 886 MB Q8_0; strong compared WER, but offline and not translation-trained |
| Whisper Medium multilingual | **Benchmark only if Turbo is unsuitable** | 769M parameters and 832 MB Q8_0; offline; slightly smaller/faster but higher WER than Turbo in Handy's three-language table |
| Whisper tiny/base and `.en` variants | **Do not replace; optional draft/offline mode only on explicit demand** | Small/fast artifacts but high multilingual WER; `.en` removes multilingual and translation coverage |
| Handy application | **Do not integrate as caption engine; reuse runtime/models selectively** | Segment-after-record microphone dictation, no live captions/system audio |
| OpenWhisper application | **Use as design reference or separate tool, not a drop-in backend** | Useful meetings/preview orchestration, but Whisper preview is window re-decode and its Python/runtime stack differs from AnyCaptions |
| OpenWhispr application | **Use as design reference, not a replacement engine** | Its Nemotron streaming path reinforces the current architecture; its Whisper sizes and Electron runtime do not create a native-streaming Whisper option |
| Riva Translate 4B + OPUS-MT | **Retain** | General text NMT and existing language-pair coverage are not replaced by Whisper speech-to-English |
| NeMo diarization | **Retain** | Dedicated streaming speaker component; standard Whisper candidates do not reproduce it |

## 7. AnyCaptions integration implications

### 7.1 Existing contracts that constrain every candidate

- `crates/anycaptions-core/src/transcription/streaming.rs` defines the 16 kHz mono `StreamingRecognizer::push`/`finish` boundary. Every `SpeechResult` carries full utterance text, `is_final`, an optional BCP-47 language, and per-word speaker tags. `StreamingProcessor` turns revisions and finals into `CaptionUpdate` values. An alternate engine must preserve those semantics rather than append independent text to the UI.
- `crates/anycaptions-core/src/transcription/session.rs` resolves one fixed Nemotron artifact and caches models only by `rnnt_right_context`. `crates/anycaptions-core/src/transcription/nemotron.rs` verifies `NEMOTRON_STREAMING_Q8`, tries Vulkan device 0 then CPU, owns the native stream, and requests word times only for diarization. Model selection and cache identity must change before a second ASR can coexist safely.
- `crates/anycaptions-core/src/transcription/diarizer.rs` feeds the same samples to the standalone NeMo diarizer. `NativeNemotron` currently owns that feed and maps final word start/end times plus estimated interim times onto speaker frames. This coupling matters for non-NeMo ASR adapters.
- `crates/anycaptions-core/src/translation/stage.rs` consumes recognition updates on a separate worker. Keep it and both `Translator` implementations unchanged for an ASR-only swap. Do not replace this stage with Whisper's speech-to-English task.

### 7.2 Recommended optional English Nemotron profile

Use the existing `NativeNemotron` path if the pinned runtime passes the compatibility test. NVIDIA's current file API reports this candidate artifact for validation, not an approved production pin:

```text
repository: nvidia/nemotron-speech-streaming-en-0.6b
revision:   ebe59e5a817142986528bbbee5dba8db7b38ed50
file:       nemotron-speech-streaming-en-0.6b.q8_0.gguf
bytes:      699872960
SHA-256:    d9a01898d2a611c8764e23a1c2f45e70bbd5a425dc4de93692ac951dd603812d
license:    NVIDIA Open Model License
```

[[NVIDIA file metadata](https://huggingface.co/api/models/nvidia/nemotron-speech-streaming-en-0.6b?blobs=true)] The candidate is 42,217,504 bytes smaller than the locked multilingual file. Handy's benchmark does not establish that it measured this exact revision, so the local A/B result remains decisive.

Required changes if the A/B gate passes:

1. **Catalog and download:** add a `ModelProfile` in `crates/anycaptions-core/src/models/catalog.rs` and a pinned source plus exact download size in `crates/anycaptions-core/src/models/download.rs`. Make it an on-demand download, not a second installer-bundled ASR. Adding both models increases total disk use even though the selected English profile is smaller.
2. **Profile selection:** generalize `NemotronPaths` and `NemotronModel::load` to verify the selected profile instead of the hard-coded multilingual constant. The low-complexity selection rule is: an explicit `en-US` or `en-GB` source plus an installed English profile selects English; Auto and every non-English source keep multilingual Nemotron. Do not switch models mid-stream after language detection.
3. **Cache identity and memory:** change `NativeProcessorFactory::asr_models` in `transcription/session.rs` to key by both model profile and right context. Release the inactive profile after its last session instead of retaining English and multilingual weights indefinitely. Captions and dictation should share the selected cached profile as they do now.
4. **Runtime compatibility:** prove that the repository's pinned NeMo-Speech.cpp v0.3.0 C API loads this exact GGUF with forced English, interim results, current right-context/EOU tuning, and word timestamps. Current upstream support is evidence of compatibility potential, not proof for the pinned DLLs. A failure must be visible; do not silently decode with a different profile.
5. **Settings and resources:** add `DownloadRow::EnglishAsr` and its controls to `crates/anycaptions-ui/src/settings/models.rs`, the corresponding state in `crates/anycaptions-core/src/presentation/downloads.rs`, and localized strings in `crates/anycaptions-core/src/presentation/i18n.rs`. The source-language picker already supplies the explicit English choice, so a second ASR-engine preference is unnecessary for this rule. Expose the active profile ID through the snapshot/resource path in `domain.rs` and `ipc.rs`; do not make the UI guess which weights loaded.
6. **Distribution and notices:** update `README.md`, `backend/models/README.md`, and license notices with the English model's NVIDIA Open Model License. Keep `packaging/artifacts.lock.json` and `packaging/installer.nsi` unchanged while the model remains download-only; add a locked payload entry only if a later decision bundles it.
7. **Regression coverage:** extend `crates/anycaptions-core/tests/real_nemotron.rs` with English-profile streaming, forced-language, interim/final, and diarizer cases. Add unit coverage for profile selection and cache-key separation in `transcription/session.rs` and download metadata tests.

No change is expected in `StreamingRecognizer`, `StreamingProcessor`, Riva/OPUS-MT, or the diarizer if this compatibility test passes.

### 7.3 Parakeet Unified or Moonshine through `transcribe.cpp`

These candidates require a new native runtime adapter and packaging set; the current NeMo DLLs do not load them. Implement `StreamingRecognizer` over the `transcribe_stream_begin/feed/finalize` API, pin the runtime and converted GGUF, and map its partial/final and timestamp behavior into `SpeechResult`.

- Parakeet Unified recomputes buffered left context; it is not the current cache-aware NeMo path. Validate the chosen `(left, chunk, right)` geometry and the 160 ms practical minimum. Its published Q8_0 file is 731 MB; Q4_K_M is 477 MB but needs a separate accuracy/latency comparison rather than inheriting Q8 results. [[Unified runtime contract](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/parakeet-unified-en-0.6b.md)]
- Moonshine Streaming needs a new stream at least every 81.9 seconds, plus explicit handling for truncation, missing VAD, missing timestamps, English-only output, and missing language detection. Those constraints make it a resource-constrained experiment, not a drop-in indefinite caption stream. [[Moonshine runtime contract](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/moonshine-streaming.md)]
- To retain speaker labels, extract the diarizer feed/attribution from `NativeNemotron` into a decorator shared by ASR adapters. Without word timestamps, final labels can use only a separately designed alignment strategy; do not copy the current interim word-estimation logic blindly.
- Add the runtime to `native.rs`, release DLLs and hashes to `packaging/artifacts.lock.json`, and add installer/update entries. This larger integration cost is why English Nemotron comes first.

### 7.4 Whisper Small, Medium, or Turbo

Do not add Whisper to the live-caption path. AnyCaptions currently has no file-transcription workflow, so an offline Whisper backend also waits for a real product use case such as file import or retranscription.

If that use case is approved:

1. Add a separate complete-audio/utterance interface instead of returning fabricated interim `SpeechResult` values. If a future buffered preview is required, own VAD, endpointing, bounded overlapping windows, seam deduplication, cancellation, and stable revision handling in its adapter. The `whisper.cpp` stream example calls its approach naive; it is not a compatibility shortcut. [[whisper.cpp stream](https://github.com/ggml-org/whisper.cpp/blob/master/examples/stream/README.md)]
2. Prefer a native `transcribe.cpp` adapter over importing OpenWhisper's Python/PyQt or OpenWhispr's Electron stack. The existing CTranslate2 OPUS-MT translator is an NMT wrapper, not a ready-made Whisper ASR interface. Budget separate runtime DLLs, conversion/provenance, revision, quantization, SHA-256, and licenses.
3. Keep engine-specific language catalogs and map Whisper language IDs to the app's BCP-47 tags. Do not reuse all 83 Nemotron prompt entries as a Whisper support claim.
4. Keep Riva and OPUS-MT as the only general text translators. Whisper multilingual models may offer direct speech-to-English, but they cannot serve the current 37-target NMT role; Turbo cannot perform even that direct translation task.
5. Disable speaker labels on a Whisper profile until word alignment exists. `transcribe.cpp` exposes segment timestamps for Whisper, while the current diarizer expects final word times and interim estimates. Preserve the existing NeMo profile when the user requests speaker labels rather than presenting guessed labels as exact.
6. Pin OpenAI's MIT model license and the actual converted-artifact license/notice. Handy's benchmark table labels Whisper Apache-2.0, but OpenAI's owning repository publishes MIT; the publisher wins that conflict. [[OpenAI license](https://github.com/openai/whisper/blob/main/LICENSE)]

### 7.5 Acceptance gate before production integration

Run the same locked multilingual baseline and candidate on identical English audio, with explicit Portuguese/French controls for any multilingual candidate. Use the production-like Windows GPU and CPU fallback, current EOU/right-context settings, and Riva plus diarization loaded together. Measure:

- WER on FLEURS and actual WASAPI meeting clips;
- first-partial latency, partial revision rate, finalization delay, and sustained long-session throughput;
- peak RAM/VRAM and disk, including both runtimes and every concurrently selected model;
- word/speaker alignment, language tags, cancellation, session restart, and translation output.

For English Nemotron, require the local accuracy/latency direction to support Handy's table without regressing the caption contract or retaining both ASR profiles after a switch. For Whisper, require an approved offline workflow and acceptable quality/latency in that workflow. Do not infer live-caption superiority from batch throughput or FLEURS alone.

## 8. Unresolved facts and source conflicts

- **Turbo parameter-count conflict:** OpenAI's current README and Handy's catalog publish **809M**, while OpenAI's separate `model-card.md` publishes **798M**. This document uses 809M in the main table because it matches the README's available-model table and the runtime catalog, but the official documentation discrepancy is not resolved here. [[README](https://github.com/openai/whisper#available-models-and-languages)] [[model card](https://github.com/openai/whisper/blob/main/model-card.md)]
- **Handy app sizes versus runtime Q8 sizes:** Handy's source resolves the app downloads as standard `ggml-small.bin` at 465 MiB, `whisper-medium-q4_1.bin` at 469 MiB, standard `ggml-large-v3-turbo.bin` at 1549 MiB, and `ggml-large-v3-q5_0.bin` at 1031 MiB. These explain the app page's rounded decimal sizes. They differ from `transcribe.cpp`'s Q8_0 GGUF artifacts by format and quantization, so packaging estimates must name the exact file. [[Handy model source](https://github.com/cjpais/Handy/blob/main/src-tauri/src/managers/model.rs#L909-L1026)] [[transcribe.cpp Whisper models](https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/whisper.md)]
- **OpenWhisper standard Whisper bytes:** public documentation says the Downloads UI displays sizes but does not enumerate each standard Local Whisper artifact. Determine the exact CTranslate2 repository, revision, and compute type before quoting an OpenWhisper download size. [[OpenWhisper model details](https://github.com/Knuckles92/OpenWhisper/blob/main/docs/speech-models.md)]
- **OpenWhispr Nemotron language count:** its local-model guide says 15 languages for multilingual Nemotron, while NVIDIA publishes 40 locales split into 32 production tiers and eight adaptation-ready tiers. The app may expose a subset or its docs may lag; no attempt is made to reconcile that product-specific list here. [[OpenWhispr guide](https://docs.openwhispr.com/guides/local-models)] [[NVIDIA language tiers](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b#supported-languages)]
- **Benchmark-to-caption mapping:** Handy's Q8/FLEURS/AMD table is the best same-method comparison found, but it does not document the complete live-caption pipeline or the production AnyCaptions hardware. Real-time ranking remains unproven until local tests run.
- **Python NeMo on native Windows:** the cited multilingual model card documents Linux for that integration. No native-Windows support claim for the Python NeMo route is made. NeMo-Speech.cpp has separate, explicit Windows support.
- **Riva Translate 4B serving route:** the model card and NeMo-Speech.cpp support the 4B model, while the current NMT NIM matrix lists only Riva Translate 1.6B and no WSL. Confirm the deployed serving path/version before applying NIM-specific hardware constraints to the existing local 4B pipeline.
- **No contrary evidence for translation or diarization replacement:** the reviewed primary sources do not justify removing Riva Translate, OPUS-MT, or NeMo diarization.

## 9. Primary-source URL list

### Handy and `transcribe.cpp`

- Handy repository: https://github.com/cjpais/Handy
- Handy model documentation: https://handy.computer/docs/models.html
- Handy FAQ (live captions/system audio): https://handy.computer/docs/faq.html
- Handy native backend configuration: https://github.com/cjpais/Handy/blob/main/src-tauri/Cargo.toml
- `transcribe.cpp` repository, backends, bindings, and catalog: https://github.com/handy-computer/transcribe.cpp
- `transcribe.cpp` Whisper contract and Q8_0 sizes: https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/whisper.md
- `transcribe.cpp` Parakeet contract: https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/parakeet.md
- `transcribe.cpp` Parakeet Unified streaming, artifacts, and measurements: https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/parakeet-unified-en-0.6b.md
- `transcribe.cpp` Moonshine Streaming contract: https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/moonshine-streaming.md
- `transcribe.cpp` multilingual Nemotron conversion/artifacts: https://github.com/handy-computer/transcribe.cpp/blob/main/docs/models/nemotron-3.5-asr-streaming-0.6b.md
- Handy first-party English Q8_0 measurements: https://models.handy.computer/languages/en/
- Handy first-party Portuguese Q8_0 measurements: https://models.handy.computer/languages/pt/
- Handy first-party French Q8_0 measurements: https://models.handy.computer/languages/fr/

### OpenWhisper

- Official repository and model/platform table: https://github.com/Knuckles92/OpenWhisper
- Model selection and acceleration guide: https://openwhisper.fiorilabs.tech/docs/choosing-a-model/
- Detailed speech-model/runtime documentation: https://github.com/Knuckles92/OpenWhisper/blob/main/docs/speech-models.md
- Preview window/final-ASR architecture: https://github.com/Knuckles92/OpenWhisper/blob/main/docs/preview-optimizations.md
- Windows installation: https://openwhisper.fiorilabs.tech/docs/installation/
- Project dependencies: https://github.com/Knuckles92/OpenWhisper/blob/main/pyproject.toml
- Privacy/offline behavior: https://openwhisper.fiorilabs.tech/docs/privacy-and-offline/

### OpenWhispr

- Official repository: https://github.com/OpenWhispr/openwhispr
- Local model engines, sizes, streaming behavior, and GPU backends: https://docs.openwhispr.com/guides/local-models
- Whisper setup, standard GGML sizes, and Windows cache/backend behavior: https://github.com/OpenWhispr/openwhispr/blob/main/LOCAL_WHISPER_SETUP.md
- Windows platform guide: https://docs.openwhispr.com/platform/windows

### OpenAI Whisper and native runtimes

- OpenAI Whisper README, parameter table, languages, tasks, and 30-second processing: https://github.com/openai/whisper
- OpenAI Whisper model card, limitations, and real-time caveat: https://github.com/openai/whisper/blob/main/model-card.md
- OpenAI Turbo model card and decoder reduction: https://huggingface.co/openai/whisper-large-v3-turbo
- OpenAI official model URLs and aliases: https://github.com/openai/whisper/blob/main/whisper/__init__.py
- OpenAI model and code license: https://github.com/openai/whisper/blob/main/LICENSE
- `whisper.cpp` standard GGML model sizes and `small.en-tdrz` caveat: https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md
- `whisper.cpp` naive buffered-stream example: https://github.com/ggml-org/whisper.cpp/blob/master/examples/stream/README.md

### NVIDIA NeMo, Riva, and OPUS-MT

- Current multilingual Nemotron 3.5 Streaming 0.6B card: https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b
- Current multilingual model artifacts: https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/tree/main
- English Nemotron Streaming 0.6B card: https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b
- English Nemotron artifacts: https://huggingface.co/nvidia/nemotron-speech-streaming-en-0.6b/tree/main
- English Nemotron exact file sizes and hashes: https://huggingface.co/api/models/nvidia/nemotron-speech-streaming-en-0.6b?blobs=true
- Parakeet Unified English model card and buffered-streaming caveat: https://huggingface.co/nvidia/parakeet-unified-en-0.6b
- Nemotron 3 Diarization card, speaker limit, and streaming configurations: https://huggingface.co/nvidia/Nemotron-3-Diarization
- NeMo cache-aware streaming architecture: https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/models.html#cache-aware-streaming-conformer
- NeMo streaming/offline diarization checkpoints: https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/speaker_diarization/results.html
- NeMo-Speech.cpp native runtime and model support: https://github.com/NVIDIA/NeMo-Speech.cpp
- NeMo-Speech.cpp Windows installation: https://github.com/NVIDIA/NeMo-Speech.cpp/blob/main/docs/install.md#windows
- Riva ASR offline/streaming/diarization behavior: https://docs.nvidia.com/deeplearning/riva/user-guide/docs/asr/asr-overview.html
- Riva SDK support matrix: https://docs.nvidia.com/deeplearning/riva/user-guide/docs/support-matrix/support-matrix.html
- NVIDIA ASR NIM support matrix: https://docs.nvidia.com/nim/speech/latest/reference/support-matrix/asr.html
- NVIDIA NMT NIM support matrix: https://docs.nvidia.com/nim/speech/latest/reference/support-matrix/nmt.html
- Riva Translate 4B Instruct v2 model card: https://huggingface.co/nvidia/Riva-Translate-4B-Instruct-v2
- OPUS-MT official repository and model resources: https://github.com/Helsinki-NLP/Opus-MT
