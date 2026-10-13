# mdviewer

Visualizador de Markdown **para Windows**, feito para uma coisa só: abrir um `.md` **instantaneamente**.

Sem editor, sem navegador, sem Electron. É um `.exe` nativo de ~9MB, sem dependências, que leva **~30ms** até mostrar o documento.

> Só funciona no **Windows 10/11 (x64)**.

## Instalação

1. Baixe o **[mdviewer.exe](https://github.com/Felipe-Meneguzzi/mdviewer/releases/latest/download/mdviewer.exe)**.
2. Execute o arquivo e responda **Sim** em "Instalar o mdviewer?".
3. O Windows abre a tela de **Aplicativos padrão**. Defina o mdviewer para `.md` e pronto: duplo clique em qualquer `.md` abre nele.

A instalação é por usuário e não precisa de admin. Ela:

- copia o exe para `%LOCALAPPDATA%\Programs\mdviewer`;
- associa `.md`, `.markdown` e `.mdown`;
- cria um atalho no Menu Iniciar;
- aparece em **Configurações → Aplicativos instalados**, de onde dá para desinstalar.

Para **atualizar**, baixe o exe novo e execute de novo. Para **desinstalar**, use Aplicativos instalados ou rode `mdviewer.exe --uninstall`.

> Se a tela de aplicativos padrão não abrir direto no mdviewer: botão direito num `.md` → **Abrir com** → **Escolher outro aplicativo** → mdviewer → **Sempre**.

## O que renderiza

Markdown no padrão do GitHub: títulos, **negrito**, *itálico*, ~~riscado~~, `código`, tabelas, listas aninhadas, checkboxes, citações, alertas (`> [!NOTE]`, `> [!WARNING]`…), linha horizontal, imagens locais e links.

Blocos de código têm syntax highlight. O tema claro/escuro segue o do Windows.

## Atalhos

| Tecla | Ação |
|---|---|
| `Ctrl+O` | Abrir arquivo |
| `Ctrl+U` / botão **Texto cru** | Alternar entre formatado e texto cru (estilo Bloco de Notas) |
| arrastar arquivo para a janela | Abrir arquivo |
| `F5` | Recarregar (também recarrega sozinho quando o arquivo é salvo) |
| `Alt+←` / `Backspace` | Voltar ao arquivo anterior |
| `PgUp` / `PgDn` / `Espaço` / setas | Rolar |
| `Home` / `End` | Início / fim |
| `Ctrl+roda` / `Ctrl +` / `Ctrl -` / `Ctrl 0` | Zoom |
| `Esc` | Fechar |

Links para outros `.md` abrem na mesma janela. URLs abrem no navegador padrão.

## Por que é rápido

Rust + [egui](https://github.com/emilk/egui) + [egui_commonmark](https://github.com/lampsitter/egui_commonmark), com render por **software (CPU)** via [egui_software_backend](https://github.com/DGriffin91/egui_software_backend).

Inicializar o driver de GPU custa mais caro do que desenhar um documento inteiro na CPU. Tempo do início do processo até o primeiro frame, medido numa GeForce GT 610:

| Backend | Tempo |
|---|---:|
| OpenGL (glow) | 230–440ms |
| wgpu / DX12 | 1.700–2.500ms |
| **Software (CPU)** | **~30ms** |

Além disso, as gramáticas de syntax highlight carregam numa thread em paralelo com a criação da janela, e o app só redesenha quando há input, então parado não usa CPU.

## Build

O projeto faz cross-compile para Windows a partir do Linux/WSL. O target padrão fica em `.cargo/config.toml`.

```sh
sudo apt install mingw-w64
rustup target add x86_64-pc-windows-gnu
cargo build --release
# → target/x86_64-pc-windows-gnu/release/mdviewer.exe
```

Cada push na `main` dispara o [GitHub Actions](.github/workflows/build.yml), que compila e publica o `.exe` na release [`latest`](https://github.com/Felipe-Meneguzzi/mdviewer/releases/latest).

## Roadmap

- [ ] Integração com WSL (`mdv arquivo.md` no terminal)
- [ ] Busca (`Ctrl+F`)
- [ ] Sumário lateral com os títulos
- [ ] Renderizar só o trecho visível em arquivos muito grandes (>100KB)
