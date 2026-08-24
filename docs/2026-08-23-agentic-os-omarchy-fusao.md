# Agentic OS (ARMS) × Omarchy — análise exaustiva e estratégia de fusão em um só sistema (v3)

> **Data**: 23/08/2026 · **v3** (3 rodadas de exploração; esta rodada: omarchy.org + Quattro 4.0.0 + Herdr + decisão "não preservar nada") · **Autor**: TACO (Claude Code + Touring 30.4.13) para Gabriel Gadea
> **Fontes primárias**: (V1) NetworkChuck, *"You need to switch to Linux RIGHT NOW!!"* (33:36, 21/08/2026) · (V2) Jay E/RoboNuggets, *"The NEW Agentic OS standard for Claude 5 Models is here"* (21:38, 21/08/2026) · (P) *ARMS — The Agentic OS Guide (Deep Edition)*, RoboNuggets, 17 p., 20/08/2026
> **Fontes oficiais**: omarchy.org (landing, manual, news), `basecamp/omarchy@quattro` (1.903 arquivos; manuais 02/05/06/07/14/17/30/31/45/47/48/49/50/51; scripts de NVIDIA/híbrido/Limine/YT6801; skill `omarchy`; plugins), release **v4.0.0 "Quattro" (14/08/2026)**, discussão #1604, **Omacom Foundation** (21/08/2026), **Herdr** (herdr.dev), Limine CONFIG.md, Hyprland wiki, docs Claude Code (`routines`, `scheduled-tasks`, `desktop-scheduled-tasks`, `desktop-linux`), getrubric.app, Hermes/OpenClaw.
> **Ground truth local**: `lsblk`, `efibootmgr`, `bootctl`, seriais NVMe, `blkid`, montagens somente-leitura (NTFS + `ext4.vhdx` via `qemu-nbd -r`), `dmidecode`, `lspci`, `nvidia-smi`.
> **Vídeo**: `claude-video-vision` — transcrição integral dos dois vídeos, 352 cortes (V2), **48 frames** inspecionados em 3 rodadas (1024 px nas rodadas 2–3). Tags: **FACT [1.0]** · **INFERENCE [0.7–0.9]** · **SPECULATION [<0.7]**.

---

## 0. Sumário executivo (v3)

1. **As duas metades**: V2/P = disciplina ARMS (Skills · Memory · Routines · Applications, 3 níveis; command centre = 20–30 % do valor); V1 = substrato Omarchy. Omarchy cobre ~60 % das afordâncias do ARMS e as 7 lacunas são *glue*. **INFERENCE 0.85**.
2. **O Omarchy que os vídeos mostram é o 4.0.0 "Quattro", lançado em 14/08/2026** — uma semana antes dos vídeos: shell inteiro reescrito em Quickshell (um processo, IPC-scriptável, substitui Waybar/Walker/Mako/SwayOSD/hyprlock/hypridle/swaybg/polkit-gnome), agente padrão + atalho + **painel de uso** + **crash → agente**, **Herdr** (`Super+Ctrl+Return`), sistema de plugins (`omarchy plugin add <git-url>`), painéis Tailscale/Dropbox, ditado local, Hyprland configurado em **Lua**, `pkexec`/polkit no lugar de `sudo` em scripts, reset de fábrica, instalador 30 % mais rápido. Em 21/08 nasceu a **Omacom Foundation** (US$ 8 M: Lütke, Collison, Dell, Dorsey, Prince, Iribe, Fried, DHH) — patrocinadora exclusiva do Hyprland. O substrato tem dinheiro e governança. **FACT 1.0**.
3. **Herdr muda o desenho**: é "tmux reconstruído para agentes" — sessões/workspaces/tabs/panes + **estados de agente** (working · blocked · done · idle · unknown) + **CLI e socket API** (`herdr agent start reviewer --kind codex --pane w1:p2`, `herdr agent prompt reviewer "…" --wait`, `herdr agent wait reviewer --until blocked`, `herdr pane read … --lines 120`), plugins, notificações, detach/reattach, acesso remoto. É o **runtime nativo para os subagents do TACO e para os nós de fan-out dos ADWs** — a "fila de agentes" que o command centre do RoboNuggets só desenha. **FACT 1.0** (herdr.dev / Flavio Copes) · **INFERENCE 0.85** (encaixe com ADW).
4. **Hardware**: Avell Storm 460, Intel iGPU + RTX 4060 Max-Q, YT6801, UEFI, **Secure Boot off**, TPM2; **dois NVMe idênticos** — `nvme0n1` **KP102L1HDJDW** (Windows, alvo) e `nvme1n1` **KP102L1H5AWP** (Pop!_OS LUKS, preservar). O instalador Quattro (full-disk + LUKS + Limine `FIND_BOOTLOADERS`) e os scripts nativos de NVIDIA/híbrido/YT6801 cobrem o seu notebook. **FACT 1.0**.
5. **Decisão registrada: "não precisa preservar nada"** — o disco Windows (927 G: 459 G Docker vhdx + 301 G WSL + caches) será apagado sem salvamento. Registro uma única vez o que deixa de existir: `projects/ANTT` (7,7 G), `archon`, `claude-engineering-kit`, `supabase` e o `.claude` do WSL (com o ADR da migração), `01_Transferegov` (8 G), `BitLocker_Keys.txt`, partição Drivers OEM. F0 vira **opcional** (um script de ~10 min, §9.3) — a decisão é sua e o plano funciona sem ele.
6. **Claude Desktop não existe no Arch** (beta, só `.deb`) → rotinas locais = `systemd --user` timers + `claude -p` (o skills deck do Jay é `claude -p /clean-up --model fable --effort xhigh --permission-mode bypassPermissions`, visto na tela); Routines cloud (`/schedule`, `/fire`) independem do Desktop. **FACT 1.0**.
7. **Estratégia v3 — "OmarchyOS Agêntico" em disco dedicado, 7 fases**: F0 (opcional) → F1 wipe + instalação → F2 skills + **skills deck como plugin Quickshell versionado** → F3 memória + Touring → F4 rotinas (timers + cloud + **Herdr como runtime de ADW**) → F5 apps (`search-connectors`, `dockur/windows`, Tailscale) → F6 command centre (3 widgets). ≈ 6 dias; Pop!_OS intocado como fallback.

---

## 1. O que cada rodada acrescentou (loop-until-dry)

| Rodada | Lentes | Achados novos | Efeito |
|---|---|---|---|
| 1 | transcrições, PDF, manual Omarchy (AI/CLI/hooks/plugins), Routines, RUBRIC/Hermes/OpenClaw | ARMS; Omarchy AI-first; Routines já existem; mapa de 16 células | tese da fusão |
| 2 | hardware/discos (ro), árvore do repo, scripts NVIDIA/híbrido, Limine #1604, unattended, scheduling, Desktop Linux, 16 frames 1024 px | 2 NVMe idênticos; Windows 927 G em vhdx; 4 projetos únicos; R5 cai; sem Desktop no Arch; `search-connectors`; comando do skills deck | estratégia em disco dedicado |
| **3** | **omarchy.org** (landing/news/hotkeys/top bar/themes/security), **release 4.0.0**, **Herdr**, **Omacom Foundation**, 16 frames | Quattro é v4.0.0 de 14/08; Herdr + API; `omarchy plugin add <git-url>`; Lua/pkexec/reset; 22 temas + 24 cores; firewall default; routine board real (8 rotinas, 3 Desktop + 5 Hermes); árvore da skill `/robo`; 60.601 arquivos | Herdr como runtime; plugin versionado; F0 opcional |

Lentes **secas** (rodaram sem novidade): PDF (3 leituras), omarchyplugins.com (0 plugins), `manual/45-troubleshooting` (sem NVIDIA/boot), `manual/49-omarchy-on` (VMs/Asahi/Steam Deck), transcrições (100 % desde a rodada 1). Ledger `touring explore` marcado; lente `external` visitada.

---

## 2. Fontes, método e cobertura

| Fonte | Como | Cobertura |
|---|---|---|
| V1 (4K AV1) | `video_analyze{transcription,silence}` + `video_detail` ×3 (12 segmentos, 24 frames) | transcrição 100 % (legendas oficiais) |
| V2 (1440p) | `video_analyze{scene_changes,silence,transcription}` + `video_detail` ×3 (12 segmentos, 24 frames) | transcrição 100 % (auto-captions), 352 cortes |
| P (17 p.) | `pypdf` integral ×3 | 100 % |
| omarchy.org | landing, `/news/…omacom-foundation…`, manual 05/06/07/48 (raw) + 02/14/17/30/31/45/47/49/50/51 (rodadas 1–2) | alta |
| `basecamp/omarchy` | árvore `gh api` + 20 arquivos raw + release v4.0.0 + #1604 | alta |
| Herdr | herdr.dev via Flavio Copes (deep dive), DEV, Buttondown | média-alta |
| Claude Code | 5 páginas de docs integrais | alta |
| Local | comandos do cabeçalho | §3 |

---

## 3. Hardware e discos — ground truth

**Máquina**: Avell STORM 460 · 32 threads · 62 GB · Intel UHD + **RTX 4060 Max-Q** (AD107M, driver 580) · ethernet **Motorcomm YT6801** · UEFI AMI 2.80 · **Secure Boot disabled** · TPM2 · Pop!_OS 24.04 (COSMIC/Wayland) com `nvidia-drm.modeset=1`. **FACT 1.0**.

| Disco | Serial (único identificador confiável) | Conteúdo | Partições |
|---|---|---|---|
| `nvme0n1` | **KP102L1HDJDW** (`/dev/disk/by-id/nvme-SM2P41C8-001TC5_KP102L1HDJDW`) | **Windows 11 — alvo do wipe** | Recovery 1,5 G · EFI 768 M · MSR · Windows 935 G NTFS (927 G usados) · Drivers OEM 16 G |
| `nvme1n1` | **KP102L1H5AWP** | **Pop!_OS — preservar** | EFI 1 G (systemd-boot, PARTUUID `10f3c3bf-ab65-4b78-b56a-3d504a05129e`) · recovery 4 G · LUKS→LVM→ext4 945 G (141 G livres) · swap |

**Conteúdo do disco Windows (lido em somente-leitura na rodada 2; decisão v3 = não preservar)**: `docker_data.vhdx` 459 G · WSL Ubuntu 24.04 `ext4.vhdx` 301 G (`/home/gabri`: `projects` 99 G — `transferegov_pipeline` 66 G e `analise` 25 G já migrados ao Pop; **únicos**: `ANTT` 7,7 G, `archon` 1,2 G, `claude-engineering-kit`, `supabase`; `.claude` 3 G com `ADR_001_WSL_TO_POPLOS_MIGRATION_SECURITY.md`; caches ~120 G) · `01_Transferegov` 8,2 G · `Downloads` 4,2 G · `BitLocker_Keys.txt` · Drivers OEM. A chave OEM do Windows fica na ACPI (`MSDM`) e sobrevive ao wipe. **FACT 1.0**.

---

## 4. V2 — RoboNuggets (consolidado das 3 rodadas)

- **Tese**: "dashboard = 20–30 %; os 70 % estão embaixo" (02:55). Frame 00:45/01:30: RUBRIC com Micro Apps (Generations, Teleprompter, Second Brain, Excalidraw), Calendar (3 fusos), YouTube Studio, Email "needs Jay", **Skills Deck** (tiles com modelo+esforço), **Routines**, artifacts ring. **FACT 1.0**.
- **Skills**: "2× → skill"; L1 `skill-creator`; L2 **skill tree — frame 07:50 mostra a pasta `/robo`**: `components/ drafts/ icons/ references/ · apps.md backgrounds.md brand-book.html courses.md infographic.md lite.md SKILL.md slides.md` (SKILL.md de 39 KB, 36 d); L3 headless — **frame 09:40**: `claude -p /clean-up --model fable --effort xhigh --permission-mode bypassPermissions` (32 s, exit 0, `C:\Users\jedoe` → **Windows**).
- **Memory**: "60.000 arquivos" (frame 12:30: "Search 60,601 files"); router files (frame 13:00 `CONTENT.md`; frame 13:30: prompt L2 na íntegra, idêntico ao PDF); second brain (anéis SKILLS→MEMORY→ROUTINES→APPLICATIONS).
- **Routines** — **frame 25:50, o routine board real**: `07:00 morning sync conflict check (DESKTOP) FIRED · 07:00 client health scan (HERMES) FIRED · 08:00 youtube to substack daily (DESKTOP) FIRED · 09:00 daily inbox digest team (DESKTOP) FIRED · 11:00 deliverables status sweep (HERMES) FIRED · 13:00 community pulse digest (HERMES) FIRED · 16:30 content pipeline check (HERMES) NEXT · 18:00 client report drafts (HERMES) QUEUED` — "6/8 fired today · CLAUDE DESKTOP 3 · HERMES 14". L3 "Coming Soon" (frame 18:10) — já existe (§7.2). **FACT 1.0**.
- **Apps**: L2 **`search-connectors`** (frame 29:15, spec integral): oficial primeiro (CLI/API/MCP do fornecedor) → comunitário na ordem CLI → API → MCP via `gh search repos "<app> cli" --sort stars --limit 5`, lista `modelcontextprotocol/servers`, variantes de nome; health check (último commit, stars, arquivado; morto > ~1 ano); 1–2 por categoria; relatório inline `Connectors for <App>` → `Official (<vendor>)` → `CLI/API/MCP: <name> — <link> — …`; termina com **uma** recomendação + oferta de instalar e provar. L3 "CLI printing press" + micro-apps; frame 21:10: pirâmide SKILLS → MEMORY → ROUTINES → APPS ("give Claude arms").
- **Crítica**: Routines L2 com segundo agente (Hermes) = duas memórias remendadas por Syncthing; `bypassPermissions` sem discussão de segurança; RUBRIC é produto de membros. **INFERENCE 0.85**.

## 5. P — ARMS Deep Edition
Inalterado (v1 §3): 4 × 3 níveis, 1 prompt por nível, 7 princípios do command centre (**show, don't store**; 3 widgets), plano de 7 dias, checklist. Texto integral em `docs/2026-08-23-agentic-os-omarchy/arms-pdf-text.txt`.

## 6. V1 — NetworkChuck (consolidado)
AI-first/"no mouse"/"sem gerenciar Linux" (00:00) · install 1 min 47 s (01:31) · Arch/DHH 3.000 h/*malleable* (02:46) · keybinds (06:48) · temas + **agente padrão + skill omarchy cria tema** (11:44) · Codex com quota 71 %, tema XP, **Pomodoro plugin** (16:33; frame 18:00: tema Cyberpunk com chuva de neon) · **crash → agente** (20:12; frame 26:10/24:40: Codex `gpt-5.6 xhigh`, Proton 11.0-1, `dockur/windows` "Downloading Windows 11 — Windows for Docker v6.04" em noVNC `127.0.0.1:8006`, `Super+T` floating) · Arch+Hyprland+Quickshell, repo 1 mês atrás, 4 canais (25:01) · "I use Arch btw" (28:06). Patrocínio ThreatLocker (não roda em Arch); "500+ plugins" vs 0 no site. **FACT 1.0**.

---

## 7. Documentação oficial (v3)

### 7.1 omarchy.org + Quattro 4.0.0 (14/08/2026) + Omacom Foundation (21/08/2026)
- **Site**: "Beautiful, Modern & Opinionated Linux by DHH"; links Manual · ISO (`omarchy-4.0.0.iso`) · Plugins · GitHub · News · Discord · Teams · Patrons · Workstations · Merch; "Incubated at 37signals"; hosting Cloudflare. **FACT 1.0**.
- **Release 4.0.0 (notas oficiais)**: shell Quickshell único; agente padrão (Claude Code, Codex, OpenCode, Pi, Oh My Pi, **Gemini**, Grok, Copilot, Crush) com `Super+Shift+Ctrl+A` e alias `a`; widget de uso (Claude/Codex/Fireworks); crash → `diagnose-crash`; **Herdr** (`Super+Ctrl+Return`, `Super+Ctrl+K` keybindings, helpers `hdl/hds/hdlm/hsl`); **plugins** `omarchy plugin add <git-url>` + *Setup > Plugins*; temas de 8 → **24 cores** com geração automática de configs de neovim/VS Code/btop; novos temas Last Horizon/Solitude/Lupine; painéis Audio/Bluetooth/Network/Display/Power; **Tailscale** (exit node) e **Dropbox**; instalador: provisioning diferido, dual boot, **Setup > Reset Computer**, ISO −1 GB, 30 % mais rápido; Omawrite (`Super+Shift+W`), Omacut, Omacalc; foot como terminal padrão; Tensaku; **Hyprland config em Lua**; NetworkManager; pacotes Arch próprios + ALPM guard (updates só via `omarchy update`); **pkexec/polkit substitui sudo**; DDC/CI; clamshell; **NVIDIA via sysfs (evita resume de D3cold)**. **FACT 1.0**.
- **Omacom Foundation** (news 21/08): nonprofit com **US$ 8 M** (8 patronos × US$ 1 M: Lütke, Collison, Dell, Dorsey, Prince, Iribe, Fried, DHH); detém marcas, financia infra e projetos upstream; **patrocinadora exclusiva do Hyprland**; "the malleable computer of the future". **FACT 1.0**.
- **Hotkeys (manual 07, integral)** — as que importam para a fusão: `Super+Space` menu · `Super+Shift+Ctrl+A` agente · **`Super+Ctrl+Return` Herdr** · `Super+Alt+Return` tmux · `Super+Ctrl+T` btop · `Super+Ctrl+X`/`F9` ditado · `Super+Ctrl+R` lembrete · `Super+Ctrl+S` LocalSend · `Super+Shift+D` LazyDocker · `Super+Shift+O` Obsidian · `Super+Ctrl+Shift+Space` tema · `Super+Ctrl+V` clipboard manager · `Super+Ctrl+Print` OCR para clipboard · `Super+Ctrl+1-9` painéis da barra · CapsLock-chords para emoji/nome/e-mail. **FACT 1.0**.
- **Top bar (manual 05)**: esquerda menu + workspaces; centro status/relógio/teclado/clima/update; direita tray, **agents** (esq. painel · dir. lança agente · meio troca assinatura), Bluetooth, rede, áudio, display, energia. `omarchy bar position bottom`, `omarchy bar move omarchy.clock --section center --index 0`, `omarchy plugin enable omarchy.media --section center`, `omarchy plugin list`. **FACT 1.0**.
- **Temas (manual 06)**: 22 (Tokyo Night, Catppuccin, Lumon, Ethereal, Everforest, Gruvbox, Miasma, Hackerman, Osaka Jade, Kanagawa, Nord, Matte Black, Vantablack, Ristretto, Retro 82, Flexoki Light, Rose Pine, Catppuccin Latte, White, Solitude, Last Horizon, Lupine); estilizam desktop, terminal, neovim, btop, Chromium e **todo o shell** (barra, menu, notificações, OSD, lock); unlock screens por tema. **FACT 1.0**.
- **Segurança (manual 48)**: LUKS **obrigatório**; firewall **ligado por padrão** (só 53317/LocalSend); SSH desligado (ativar em *Setup > Security > SSHD*, rate-limited); `ufw-docker`; sudo com senha (opção passwordless 15 min com aviso explícito); pacotes Arch + repo Omarchy; ISO assinada (`40DFB630FF42BCFFB047046CF0134EE680CAC571`). **FACT 1.0**.
- **AI (manual 17), CLI (14), hooks, plugins, menu, updates, instalação, NVIDIA/híbrido/YT6801, Limine/#1604, unattended `cidata`**: como nas v1/v2 (§5.1/§7.1 anteriores) — resumidos na tabela §8.

### 7.2 Herdr (herdr.dev; ships no Quattro)
Rust, binário único, cliente-servidor, sem telemetria. **Modelo**: Sessions → Workspaces (1 por projeto) → Tabs → Panes → **Agents** (processos reconhecidos: Codex, Claude Code, Cursor CLI, Pi, OpenCode, Copilot CLI, Devin…). **Estados**: working · blocked · done · idle · unknown (detecção por processo em foreground, "screen manifest" e integrações oficiais; estados sobem para o workspace). **CLI/API**: `herdr workspace create --cwd ~/project --label p` · `herdr tab create` · `herdr pane split` · `herdr pane run w1:p2 "npm test"` · `herdr pane read w1:p2 --source recent-unwrapped --lines 120` · `herdr agent start reviewer --kind codex --pane w1:p2` · `herdr agent prompt reviewer "Review the diff" --wait` · `herdr agent wait reviewer --until blocked` · `herdr agent explain reviewer`; socket local para scripts; **agentes podem orquestrar outros agentes**. Prefixo `ctrl+b`; `~/.config/herdr/config.toml`; notificações (toast/terminal/OS); plugins `herdr-plugin.toml`; persistência: detach/reattach (processos vivos), restart do servidor (layout volta, processos não), resume nativo de sessões em integrações; remoto via SSH/thin client/phone. **Não faz**: isolamento de arquivos (worktrees são seu problema), memória compartilhada, garantia de detecção. Instalação: `curl -fsSL https://herdr.dev/install.sh | sh` / `mise use -g herdr`. **FACT 1.0** (fonte secundária de alta qualidade; **FACT 0.9**).

### 7.3 Claude Code — 3 modos de agendamento
| | Cloud Routines | Desktop tasks | `/loop` / CronCreate |
|---|---|---|---|
| Roda | nuvem Anthropic | sua máquina (app aberto) | sua máquina (sessão aberta) |
| Persistência | sim | sim + catch-up 7 d | `--resume`; **expira em 7 d** |
| Mín. | **1 h** | 1 min | 1 min |
| Triggers | schedule · **API `/fire`** · **GitHub** | schedule | cron |
| Linux | — | **só .deb (beta)** → não no Arch | sim |
**FACT 1.0**.

---

## 8. Mapa ARMS ↔ Omarchy Quattro ↔ Touring (v3)

| ARMS | Nível | Pede | Omarchy 4.0 já dá | Touring / sua stack | Lacuna (glue) |
|---|---|---|---|---|---|
| S | L1 | pré-built + criar | skill `omarchy`, `diagnose-crash` | 175 skills, TACO-skilling | — |
| S | L2 | skill tree | SKILL.md router p/ 6 guias | REGRA #13 | auditar > 150 L (modelo: pasta `/robo`) |
| S | L3 | skill de botão | `omarchy agent prompt`; widget `bar.run`; menu JSON; **Herdr `agent prompt --wait`** | `claude -p`, `touring adw run` | **plugin `gabriel.skills-deck` versionado (`omarchy plugin add <git-url>`)** |
| M | L1 | pasta | `~/Work` | `~/projects` | — |
| M | L2 | router files | — | CLAUDE.md + rules + índice + `memory moc` | **router por área de vida** |
| M | L3 | second brain | — | `memory moc --out`, portfolio | **1 HTML local como web app** |
| R | L1 | routine local | hooks `.d`, `omarchy reminder` (`Super+Ctrl+R`) | crons, timers, scout, ADW journal | **routine board em `systemd --user`** (sem Desktop no Arch) |
| R | L2 | always-on | painel funde uso via `syncDir`; **painel Tailscale nativo**; Herdr remoto | daemon Touring, ADW `--resume-run` | Syncthing/Tailscale + máquina 24/7 (opcional) |
| R | L3 | nuvem | — | — | **Routines** (`/schedule`, `/fire` de hook) |
| A | L1 | conectores | web apps/TUIs; Install > AI; Dropbox | 4 MCP + por projeto | connector audit |
| A | L2 | agente busca | — | portfolio, Context7, Exa | **skill `search-connectors`** |
| A | L3 | construir | `omarchy-mise-install`; webapp | **cli-anything** | — |
| CC | — | 1 página | barra + painéis + **Herdr sidebar** | `touring status/kpi`, Artifacts | **dashboard HTML (3 widgets)** |

---

## 9. Estratégia v3 — "OmarchyOS Agêntico" em disco dedicado, com Herdr como runtime

### 9.1 Princípios
1. Um agente de verdade (Claude Code + seus skills/rules), várias superfícies (Omarchy · Routines · **Herdr**); Touring = sistema nervoso. 2. Show, don't store = Lei L3 (Herdr `pane read` é evidência em disco). 3. Afordância > persuasão (keybind/hook/widget/ADW). 4. Deny-by-default (launchers auto-approve → `--permission-mode` explícito, CEG, `~/Work`). 5. Bottom-up. 6. Isolamento físico (Omarchy no `nvme0n1`, Pop no `nvme1n1`). 7. Reprodutível (`cidata` + plugin repo + `routines.toml` versionados).

### 9.2 Camadas
```
L5  COMMAND CENTRE   dashboard HTML (3 widgets) como web app · barra Quickshell (agents/usage, routines, skills deck) · menu extensions · Herdr sidebar
L4  ARMS DISCIPLINE  skill tree · router files por área · do-it-twice · search-connectors · connector audit · 7-day plan
L3  NERVOUS SYSTEM   Touring daemon · memory moc · ADW runner+journal (fan-out → Herdr agents) · scout perpétuo · CEG · kpi
L2  AGENT HARNESS    Claude Code default + mise (codex/opencode/pi/gemini/ori) · Herdr (sessions, estados, API) · claude -p · Routines cloud · systemd --user timers
L1  OMARCHY SHELL    Hyprland (Lua) · Quickshell · omarchy CLI · hooks *.d · temas 24 cores · web apps/TUIs · Tailscale panel · dockur/windows
L0  BASE             nvme0n1 KP102L1HDJDW: Arch/Omarchy 4.0 stable · LUKS · btrfs+snapper · Limine (+Pop via limine-scan) · nvidia-open-dkms · yt6801-dkms · Docker/KVM
                     nvme1n1 KP102L1H5AWP: Pop!_OS intocado (fallback ≥ 30 dias)
```

### 9.3 F0 — opcional (decisão v3: não preservar)
Você decidiu não preservar. Se mudar de ideia, este bloco leva ~10 min e salva só o que é único (≈ 12 GB): 
```bash
sudo mount -o ro,noexec /dev/disk/by-id/nvme-SM2P41C8-001TC5_KP102L1HDJDW-part4 /mnt/win
sudo modprobe nbd max_part=8 && sudo qemu-nbd -r -c /dev/nbd0 '/mnt/win/Users/gabri/AppData/Local/wsl/{ae2b9a0a-01de-465a-bfa6-880790814cb5}/ext4.vhdx' && sudo mount -o ro,noexec /dev/nbd0 /mnt/wsl
rsync -a /mnt/wsl/home/gabri/projects/{ANTT,archon,claude-engineering-kit,supabase} /mnt/wsl/home/gabri/.claude ~/salvage-win/
rsync -a /mnt/win/Users/gabri/01_Transferegov /mnt/win/BitLocker_Keys.txt ~/salvage-win/
sudo umount /mnt/wsl && sudo qemu-nbd -d /dev/nbd0 && sudo umount /mnt/win
```
Sem F0, o plano segue direto para F1.

### 9.4 F1 — wipe + instalação (½ dia)
1. `caligula` grava `omarchy-4.0.0.iso` (verificar `.sig` com a chave `40DFB630…`). Secure Boot já está desligado.
2. **Tornar o alvo inequívoco**: `sudo wipefs -a /dev/disk/by-id/nvme-SM2P41C8-001TC5_KP102L1HDJDW` (destrutivo — é o wipe que você autorizou); no instalador, o único disco sem partições é o alvo.
3. Instalador Quattro: **full-disk** no disco vazio, **LUKS sim** (teclado interno), usuário, `America/Sao_Paulo`. 2–5 min. Reprodutível depois via `cidata` (`user_configuration.json` com o disco por `by-id`).
4. Primeiro boot: `omarchy default agent claude` (`cx` autentica); conferir `pacman -Q nvidia-open-dkms yt6801-dkms`, `/etc/modprobe.d/nvidia.conf`; `omarchy toggle hybrid-gpu` só se suspend/bateria incomodar.
5. Pop!_OS no menu: `sudo limine-entry-tool --scan` (#1604); fallback `protocol: efi` + `path: guid(10f3c3bf-ab65-4b78-b56a-3d504a05129e):/EFI/systemd/systemd-bootx64.efi`; `cp /boot/limine.conf{,.bak}`. Firmware: Limine 1º, Pop 2º ("UEFI Hard Disk BBS Priorities").
6. `Setup > Security`: manter sudo com senha; SSH off; `Install > AI > Dictation` se quiser ditado (`Super+Ctrl+X`).
**Validador**: `omarchy debug --no-sudo --print` limpo · `nvidia-smi` · YT6801 em `ip link` · Limine lista os 2 OS · `claude -p "ok"` · `herdr --version`.

### 9.5 F2 — Skills + skills deck como plugin (1 dia)
- Copiar `~/.claude/{skills,rules,agents,settings.json}` do Pop (montar o LUKS do Pop **ro** a partir do Omarchy ou via pendrive). A skill `omarchy` chega por symlink.
- **Plugin `gabriel.skills-deck`** (repo git próprio, instalado com `omarchy plugin add <git-url>`): `manifest.json` (`kinds:["bar-widget"]`, `schema`: `model` enum, `effort` enum, `skills` lista) + `Widget.qml` com `bar.run("claude -p '/<skill>' --model <m> --effort <e> --permission-mode acceptEdits >> ~/Work/runs.log")` ou, para trabalho longo, `bar.run("herdr agent start <skill> --kind claude --pane w1:p1 && herdr agent prompt <skill> '/<skill>'")`. Menu: `"taco.audit": {"action": "omarchy agent prompt '/TACO-cross-audit'"}`.
**Validador**: `omarchy plugin list` mostra o plugin; 1 botão gera artefato + linha em `runs.log`.

### 9.6 F3 — Memória + Touring (1–2 dias)
`~/Work/CLAUDE.md` router por área (touring · ANTT/DETRAN · conteúdo · pessoal) + `AREA.md` < 1 página; `update-touring` + toolchain; hooks `post-boot.d/20-touring` (`touring daemon-ctl status`), `post-update.d/30-touring-doctor`; second brain: `touring memory moc --out` → 1 HTML auto-contido → `omarchy webapp install "Second Brain" http://localhost:7777/brain`. Tema: os 24 colorsets já geram configs; Claude Code segue o tema automaticamente.
**Validador**: teste cego 3 arquivos/1º salto; `touring doctor -j` 5/5.

### 9.7 F4 — Rotinas + Herdr como runtime de ADW (1 dia)
- **Routine board** (`~/Work/routines.toml` → gerador de `systemd --user` timers com `Persistent=true`, `claude -p`, log 1 linha/run) — o modelo é o board do frame 25:50 (hora · rotina · runner · status), com runners `local` (timer) e `cloud` (Routine).
- 1 Routine cloud (`/schedule`: docs-drift semanal no repo touring) + 1 trigger `/fire` disparado por hook `post-update.d` ("o update quebrou o build do touring? abra PR").
- **ADW × Herdr**: nó `parallel` do ADW cria `herdr workspace` por ramo, `herdr agent start` + `agent prompt --wait`, `herdr agent wait --until done|blocked`, `herdr pane read` → artefato no journal (Lei L3). Estados `blocked` viram notificações da barra. **INFERENCE 0.85** (API documentada; integração é trabalho seu).
**Validador**: `systemctl --user list-timers`; 1 run cloud com PR `claude/*` com o laptop fechado; 1 ADW com 2 ramos em Herdr terminando `done`.

### 9.8 F5 — Apps (1 dia)
Skill **`search-connectors`** (spec §4) + connector audit (≤ 3); web apps (Gmail, Calendar, Notion, claude.ai/code/routines) + TUIs (`touring`, `btop`, `lazydocker`); `cli-anything` + `omarchy-mise-install` para apps sem conector; **`dockur/windows`** (Docker + KVM, noVNC `127.0.0.1:8006`) para o Windows eventual — sem ativação automática da licença OEM; Tailscale pelo painel nativo se houver máquina 24/7.
**Validador**: 1 chamada read-only por conector; Windows abre no noVNC.

### 9.9 F6 — Command centre (1 dia)
`~/Work/cc/index.html` + `cc.json` (`touring status/kpi`, `~/.local/state/omarchy/agents/usage/`, `routines.toml`, `~/Work/artifacts/`) servido por `systemd --user`, registrado como web app; **3 widgets** (artefatos, routine board, skills deck) — a barra já cobre uso de agentes e o Herdr a fila de agentes.
**Validador**: sobrevive a reboot; abre por `Super+Space cc`.

### 9.10 O que não fazer
Não confiar em `nvme0/1` (só serial) · não manter Windows em partição (Docker/KVM cobre) · não converter `.deb` do Claude Desktop para Arch · não usar `bypassPermissions` por padrão · não rodar a skill `omarchy` fora de plan mode na 1ª semana · não editar `/usr/share/omarchy` · não usar `sudo` em scripts de background (Quattro usa `pkexec`) · não adotar Hermes/OpenClaw como 2º cérebro · não comprar RUBRIC para começar.

---

## 10. Riscos e decisões (v3)

| # | Risco | Nível | Mitigação |
|---|---|---|---|
| R1 | Wipe do disco errado (modelos idênticos) | alto se ignorado | by-id/serial; `wipefs` prévio; conferir no instalador |
| R2 | Dados únicos do WSL perdidos (ANTT, archon, .claude) | **aceito por decisão** | F0 opcional de 10 min, se quiser |
| R3 | Launchers em auto-approve | médio | `--permission-mode` explícito; CEG; `~/Work` |
| R4 | Routines cloud em preview (1 h, cap, só GitHub) | médio | timers locais como equivalente |
| R5 | NVIDIA híbrida | **baixo** | scripts nativos; Quattro detecta via sysfs (sem D3cold resume bug) |
| R6 | Sem Claude Desktop no Arch | médio | timers `systemd --user` |
| R7 | Update regenera `limine.conf` | baixo | `.bak` + `limine-scan` pós-update |
| R8 | Quattro é v4.0.0 com 9 dias; Hyprland em Lua, pkexec, pacotes próprios — **superfície nova** | médio | canal `stable` (1 mês atrás); snapshots btrfs; `Setup > Reset Computer` |
| R9 | Herdr jovem; detecção de estado "não garantida" | médio | usar `agent wait` só como sinal; veredito continua sendo artefato (Lei L3) |
| R10 | Skill `omarchy` experimental · marketplace vazio · Arch × daemon Touring · OEM não ativa no VM | baixo | plan mode · widgets próprios · hook `post-update.d → touring doctor` · aceitar |

**Decisões que só você toma**: (a) **autorizar o wipe do `nvme0n1` KP102L1HDJDW** (já declarou que nada precisa ser preservado — confirmar o comando `wipefs` é o gatilho); (b) passphrase LUKS e GPU inicial (Hybrid); (c) destino do Pop!_OS após 30 dias; (d) 3 primeiras rotinas na nuvem; (e) nome/repo do plugin `skills-deck`.

---

## 11. Confiança (amostra)

| Afirmação | Tag | Evidência |
|---|---|---|
| Quattro = v4.0.0, 14/08/2026; features listadas | FACT 1.0 | release notes GitHub; omarchy.org |
| Omacom Foundation US$ 8 M, 21/08/2026, patrocinadora do Hyprland | FACT 1.0 | omarchy.org/news |
| Herdr: estados + CLI + socket API | FACT 0.9 | herdr.dev via deep dive; release notes |
| Hotkeys/top bar/temas/segurança | FACT 1.0 | manual 05/06/07/48 |
| `nvme0n1` KP102L1HDJDW = Windows; conteúdo | FACT 1.0 | medido em ro |
| Routine board do Jay (8 rotinas, 3 Desktop + 5 Hermes) | FACT 1.0 | frame 25:50 |
| Herdr como runtime de fan-out ADW | INFERENCE 0.85 | API documentada; integração a construir |
| `limine-entry-tool --scan` acha o Pop | INFERENCE 0.8 | #1604 + `FIND_BOOTLOADERS` |
| "500+ plugins" → outro diretório | SPECULATION 0.5 | site mostra 0 |

---

## 12. Fontes
**Vídeos**: https://youtu.be/9SDkU5VDQEQ · https://youtu.be/8NSyI-npJCU · **PDF**: `docs/ARMS-Agentic-OS-Guide.pdf`
**Omarchy**: https://omarchy.org/ · https://omarchy.org/manual · https://omarchy.org/news/2026/08/omacom-foundation-launches-with-8-million/ · https://omarchy.org/news/2026/08/omacom-foundation-to-be-exclusive-hyprland-sponsor/ · https://github.com/basecamp/omarchy/releases/tag/v4.0.0 · https://github.com/basecamp/omarchy/pull/6231 · https://github.com/basecamp/omarchy (manuais 02/05/06/07/14/17/30/31/45/47/48/49/50/51; `install/hardware/*`, `bin/omarchy-*`, `default/hypr/nvidia.lua`, `default/limine/*`, `etc/limine-entry-tool.d/*`, `default/agents/skills/omarchy/*`, `shell/plugins/*`) · https://github.com/basecamp/omarchy/discussions/1604 · https://omarchyplugins.com · https://www.phoronix.com/news/Omarchy-4.0-Released
**Herdr**: https://herdr.dev · https://flaviocopes.com/herdr/ · https://dev.to/pvgomes/herdr-is-tmux-for-coding-agents-3ai0
**Hyprland / Limine**: https://wiki.hypr.land · https://github.com/limine-bootloader/limine/blob/v12.x/CONFIG.md
**Claude Code**: https://code.claude.com/docs/en/routines · /scheduled-tasks · /desktop-scheduled-tasks · /desktop-linux · /best-practices
**Ecossistema**: https://www.getrubric.app/ · https://github.com/NousResearch/hermes-agent · https://www.digitalocean.com/resources/articles/what-is-openclaw
**Artefatos**: `docs/2026-08-23-agentic-os-omarchy/` (transcrições, PDF, frames das 3 rodadas, `omarchy_tree.txt`).
