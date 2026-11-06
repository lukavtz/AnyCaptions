# Modelos

Arquivos de modelo (GGUF) organizados por capacidade. Eles não são versionados;
a fonte da verdade sobre cada modelo (origem, revisão, licença e SHA-256) é o
catálogo em `crates/anycaptions-core/src/models/catalog.rs`, que verifica o arquivo antes de carregar.

```
models/
  asr/            # transcrição de fala
    <modelo>/     # variantes de quantização compartilham a pasta
  translation/    # tradução de texto
  diarization/    # identificação de falantes (rótulos genéricos "Pessoa N")
```

O ASR multilíngue continua sendo o padrão. O perfil opcional somente em inglês
(`asr/nemotron-speech-streaming-en-0.6b/nemotron-speech-streaming-en-0.6b.q8_0.gguf`)
é baixado dentro do app e usado somente quando a fonte explícita é `en-US` ou
`en-GB` e o arquivo está instalado. Automático e todos os outros idiomas usam o
ASR multilíngue. A seleção ocorre ao abrir a sessão, nunca durante o fluxo.

O modelo de diarização (`diarization/nemotron-3-diarization/Nemotron-3-Diarization.q8_0.gguf`)
é opcional: baixado dentro do app pelas configurações ("Identificação de
falantes"), direto do repositório da NVIDIA na revisão fixada no catálogo.
Sem ele, a transcrição funciona normalmente, sem rótulos.

Perfil inglês fixado:
- Repositório: `nvidia/nemotron-speech-streaming-en-0.6b`
- Revisão: `ebe59e5a817142986528bbbee5dba8db7b38ed50`
- Tamanho exato: `699872960` bytes
- SHA-256: `d9a01898d2a611c8764e23a1c2f45e70bbd5a425dc4de93692ac951dd603812d`
- Licença: [NVIDIA Open Model License](https://www.nvidia.com/en-us/agreements/enterprise-software/nvidia-open-model-license/)

O tamanho e o SHA-256 são verificados após o download e antes de cada carga.
Este perfil não entra no instalador nem no manifesto de pacote.

A raiz pode ser trocada com a variável de ambiente `ANYCAPTIONS_MODELS_DIR`.
