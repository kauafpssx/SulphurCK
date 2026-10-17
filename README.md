<h1 align="center">⚡ SulphurCK</h1>

<p align="center">
  <strong>O motor de configs no estilo SilverBullet/OpenBullet, reimaginado para o terminal.</strong><br />
  CLI leve, concorrente e nativa — escrita em Rust.
</p>

<p align="center">
  <a href="https://github.com/kauafpssx/SulphurCK/actions"><img src="https://github.com/kauafpssx/SulphurCK/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="https://github.com/kauafpssx/SulphurCK/releases"><img src="https://img.shields.io/github/v/release/kauafpssx/SulphurCK?display_name=tag&style=flat&color=7c3aed" alt="Release" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS-2ea44f?style=flat" alt="Windows, Linux e macOS" />
  <a href="LICENSE"><img src="https://img.shields.io/github/license/kauafpssx/SulphurCK?style=flat&color=22c55e" alt="MIT" /></a>
</p>

> [!WARNING]
> Use apenas em sistemas, contas e endpoints para os quais você tem autorização explícita. O projeto não deve ser usado para acesso não autorizado, credential stuffing ou violação de termos de serviço.

<p align="center">
  <a href="#-compatibilidade">Compatibilidade</a> ·
  <a href="#-instalação-rápida">Instalação</a> ·
  <a href="#-uso">Uso</a> ·
  <a href="#-arquitetura">Arquitetura</a> ·
  <a href="#-qualidade-e-cicd">CI/CD</a>
</p>

## ✨ O que ele faz

SulphurCK é um checker HTTP nativo inspirado no ecossistema **SilverBullet/OpenBullet**. Ele lê fluxos LoliScript voltados a HTTP, processa wordlists concorrentemente e opera inteiramente pelo terminal — ideal para testes autorizados, pipelines e máquinas de poucos recursos.

| Recurso | Descrição |
| --- | --- |
| 🦀 **Nativo e leve** | Binário Rust; TLS via `rustls`, sem dependência de OpenSSL. |
| 🌐 **HTTP + proxies** | HTTP(S), SOCKS4/SOCKS4a/SOCKS5, cookies e headers. |
| 🧩 **Motor de script** | Subconjunto HTTP do LoliScript: `REQUEST`, `KEYCHECK`, `PARSE`, `FUNCTION`, `UTILITY` e `TCP`. |
| 📊 **Operação no terminal** | CLI para automação, TUI para acompanhamento e base para daemon/IPC. |
| 🔔 **Resultados** | Hits estruturados, saída em arquivo e webhook opcional. |
| 🖥️ **Multiplataforma** | Build nativo e teste contínuo em Windows, Linux e macOS. |

## 🧩 Compatibilidade

SulphurCK prioriza o formato de configuração conhecido por usuários de **SilverBullet** e **OpenBullet**, sem copiar interface gráfica ou depender de navegador.

| Arquivo/origem | Status | Observação |
| --- | --- | --- |
| SilverBullet `.svb` | ✅ | Lê `[SETTINGS]` e `[SCRIPT]`. |
| OpenBullet / SilverBullet `.loli` | ✅ | Aceita LoliScript puro, sem seções, com configurações padrão. |
| Blocos HTTP | ✅ | `REQUEST`, `KEYCHECK`, `PARSE`, `FUNCTION`, `UTILITY` e `TCP`. |
| Browser, Selenium, OCR e CAPTCHA | ❌ | Fora de escopo: SulphurCK é deliberadamente HTTP-only. |

Antes de rodar uma configuração importada, valide-a:

```bash
sulphurck config validate minha-config.loli
```

Blocos não implementados são identificados no log e ignorados; isso evita afirmar compatibilidade completa quando uma configuração depende de automação de navegador.

## 🛡️ Proxies remotos e serviços

Cadastre uma fonte por protocolo. O SulphurCK baixa a lista novamente antes de cada `run`, valida as entradas, atualiza o cache local e usa rotação round-robin durante a execução.

```bash
sulphurck proxy add http https://exemplo.com/http.txt
sulphurck proxy add socks5 https://exemplo.com/socks5.txt
sulphurck proxy add socks4 https://exemplo.com/socks4.txt
sulphurck proxy list
sulphurck proxy refresh
```

Para manter mais de uma execução sem TUI, use serviços nomeados:

```bash
sulphurck service start loja-a --config loja-a.svb --wordlist dados-a.txt
sulphurck service start loja-b --config loja-b.loli --wordlist dados-b.txt --bots 2
sulphurck service list
sulphurck service logs loja-a
sulphurck service stop loja-a
```

Cada serviço recebe PID e log próprio. Sem `--bots`, o serviço usa `SuggestedBots` da configuração SilverBullet/OpenBullet; use `--bots` somente para sobrescrever. Sem `--proxies`, todo serviço usa todas as fontes remotas ativas, atualizadas antes de iniciar. As fontes e o cache ficam no diretório de dados local do sistema, fora do repositório.

## 🚀 Instalação rápida

### A partir de uma release

Baixe o pacote da sua plataforma na página de Releases, extraia-o e execute:

```bash
# Linux / macOS
./sulphurck --help

# Windows (PowerShell)
.\sulphurck.exe --help
```

### Compilando do código-fonte

Pré-requisito: [Rust stable](https://rustup.rs/) com `cargo`.

```bash
git clone https://github.com/kauafpssx/SulphurCK.git
cd SulphurCK
cargo run -- --help
```

Para criar o binário otimizado:

```bash
cargo build --release
```

O executável fica em `target/release/` (`sulphurck.exe` no Windows).

## 🧭 Uso

```bash
# Ver todos os comandos
sulphurck --help

# Executar um fluxo autorizado
sulphurck --config config.svb --wordlist data.txt --bots 4 run --max-cpm 60

# Validar o arquivo antes de executar
sulphurck config validate config.svb

# Inspecionar uma lista de proxies
sulphurck proxy parse proxies.txt

# Utilitários de wordlist
sulphurck wordlist count data.txt
sulphurck wordlist unique data.txt

# Abrir a interface de terminal
sulphurck tui
```

O comando `run` é o padrão quando `--config` é informado. Os valores de `config.svb`, `data.txt`, `hits.txt` e configurações locais são ignorados pelo Git de propósito: podem conter dados sensíveis ou específicos de cada ambiente.

## 🏗️ Arquitetura

```mermaid
flowchart LR
    A[CLI / TUI / Daemon] --> B[Runner\ncaso de uso]
    B --> C[Engine + LoliScript\nregras de execução]
    B --> D[Dados, hits e configuração]
    C --> E[HTTP, proxy e webhook\nadaptadores de infraestrutura]
```

O núcleo usa uma **Clean Architecture pragmática**: as interfaces de terminal e rede ficam nas bordas; parser, motor e modelos permanecem independentes delas. Consulte [a arquitetura](/docs/ARCHITECTURE.md) para regras, limites atuais e o plano de evolução.

```text
src/
├── main.rs               # composition root mínimo
├── domain/               # configuração e linguagem de script, sem I/O
├── application/          # engine e runner (casos de uso)
├── infrastructure/       # HTTP, arquivos, proxies, hits, IPC e webhooks
└── presentation/         # adaptadores CLI e TUI
```

## ✅ Qualidade e CI/CD

Todo push e pull request para `main` executa:

1. `cargo fmt --check`
2. `cargo clippy -- -D warnings`
3. Testes unitários
4. Build e smoke test nativos em Ubuntu, Windows e macOS

Todo push concluído em `main` cria a próxima versão automaticamente, gera o commit e a tag, e publica uma GitHub Release com binários por plataforma. O incremento é sequencial por patch: `0.0.1 → 0.0.2 → … → 0.0.9 → 0.1.0`. Veja o workflow em [`.github/workflows/ci.yml`](/.github/workflows/ci.yml).

Para reproduzir localmente:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --locked --release
```

## 🤝 Contribuindo

1. Crie uma branch curta e focada.
2. Adicione um teste quando alterar comportamento.
3. Execute o conjunto de qualidade acima antes de abrir o pull request.
4. Mantenha as dependências apontando para dentro, conforme a [arquitetura](/docs/ARCHITECTURE.md).

Issues e PRs devem descrever o comportamento esperado, como reproduzir e o impacto de compatibilidade.

## 📄 Licença

Distribuído sob a [licença MIT](/LICENSE).
