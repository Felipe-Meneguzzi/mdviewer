//! Instalação do próprio .exe, por usuário (HKCU, sem precisar de admin).
//!
//! - copia o exe para `%LOCALAPPDATA%\Programs\mdviewer`
//! - associa .md/.markdown/.mdown ("Abrir com" e app padrão)
//! - cria atalho no Menu Iniciar
//! - registra em "Aplicativos instalados" para desinstalar pelo Windows

use std::path::PathBuf;

const APP: &str = "mdviewer";
const PROG_ID: &str = "mdviewer.md";
const EXTS: [&str; 3] = [".md", ".markdown", ".mdown"];
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\mdviewer";
const CAPABILITIES_KEY: &str = r"Software\mdviewer\Capabilities";

fn install_dir() -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("Programs")
            .join(APP),
    )
}

fn shortcut_path() -> Option<PathBuf> {
    Some(
        PathBuf::from(std::env::var_os("APPDATA")?)
            .join(r"Microsoft\Windows\Start Menu\Programs\mdviewer.lnk"),
    )
}

/// Executado sem argumentos e fora da pasta de instalação: oferece instalar.
/// Retorna `true` se instalou (o processo deve encerrar).
pub fn offer_install() -> bool {
    let (Ok(exe), Some(dir)) = (std::env::current_exe(), install_dir()) else {
        return false;
    };
    let installed = dir.join("mdviewer.exe");
    if same_file(&exe, &installed) {
        return false;
    }
    let msg = if installed.exists() {
        "Já existe uma instalação do mdviewer.\n\nAtualizar com esta versão?"
    } else {
        "Instalar o mdviewer neste computador?\n\n\
         Ele será copiado para %LOCALAPPDATA%\\Programs\\mdviewer e \
         registrado para abrir arquivos .md."
    };
    let answer = rfd::MessageDialog::new()
        .set_title(APP)
        .set_level(rfd::MessageLevel::Info)
        .set_buttons(rfd::MessageButtons::YesNo)
        .set_description(msg)
        .show();
    if answer != rfd::MessageDialogResult::Yes {
        return false;
    }
    install();
    true
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn info(msg: &str) {
    rfd::MessageDialog::new()
        .set_title(APP)
        .set_level(rfd::MessageLevel::Info)
        .set_description(msg)
        .show();
}

fn error(msg: &str) {
    rfd::MessageDialog::new()
        .set_title(APP)
        .set_level(rfd::MessageLevel::Error)
        .set_description(msg)
        .show();
}

#[cfg(windows)]
pub fn install() {
    match install_inner() {
        Ok(()) => {
            info(
                "mdviewer instalado!\n\n\
                 A seguir abrirá a tela de aplicativos padrão do Windows: \
                 defina o mdviewer para .md.\n\n\
                 Se ela não abrir direto no mdviewer: botão direito num .md → Abrir com → \
                 Escolher outro aplicativo → mdviewer → Sempre.",
            );
            open_default_apps_settings();
        }
        Err(e) => error(&format!("Falha na instalação: {e}")),
    }
}

#[cfg(windows)]
fn install_inner() -> std::io::Result<()> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let dir = install_dir().ok_or_else(|| std::io::Error::other("LOCALAPPDATA não definido"))?;
    std::fs::create_dir_all(&dir)?;
    let target = dir.join("mdviewer.exe");
    let current = std::env::current_exe()?;
    if !same_file(&current, &target) {
        std::fs::copy(&current, &target)?;
    }
    let exe = target.to_string_lossy();
    let open_cmd = format!("\"{exe}\" \"%1\"");

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let classes = hkcu.create_subkey(r"Software\Classes")?.0;

    let (prog, _) = classes.create_subkey(PROG_ID)?;
    prog.set_value("", &"Documento Markdown")?;
    prog.create_subkey("DefaultIcon")?
        .0
        .set_value("", &format!("\"{exe}\",0"))?;
    prog.create_subkey(r"shell\open\command")?
        .0
        .set_value("", &open_cmd)?;

    let (app, _) = classes.create_subkey(r"Applications\mdviewer.exe")?;
    app.set_value("FriendlyAppName", &APP)?;
    app.create_subkey(r"shell\open\command")?
        .0
        .set_value("", &open_cmd)?;
    let (types, _) = app.create_subkey("SupportedTypes")?;

    // Capabilities + RegisteredApplications: faz o mdviewer aparecer em "Aplicativos padrão".
    let (caps, _) = hkcu.create_subkey(CAPABILITIES_KEY)?;
    caps.set_value("ApplicationName", &APP)?;
    caps.set_value("ApplicationDescription", &"Visualizador rápido de Markdown")?;
    let (assoc, _) = caps.create_subkey("FileAssociations")?;
    hkcu.create_subkey(r"Software\RegisteredApplications")?
        .0
        .set_value(APP, &CAPABILITIES_KEY)?;

    for ext in EXTS {
        classes
            .create_subkey(format!(r"{ext}\OpenWithProgids"))?
            .0
            .set_value(PROG_ID, &"")?;
        types.set_value(ext, &"")?;
        assoc.set_value(ext, &PROG_ID)?;
    }

    let (un, _) = hkcu.create_subkey(UNINSTALL_KEY)?;
    un.set_value("DisplayName", &APP)?;
    un.set_value("DisplayIcon", &format!("\"{exe}\",0"))?;
    un.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION"))?;
    un.set_value("Publisher", &"Felipe Meneguzzi")?;
    un.set_value("InstallLocation", &dir.to_string_lossy().into_owned())?;
    un.set_value("UninstallString", &format!("\"{exe}\" --uninstall"))?;
    un.set_value("NoModify", &1u32)?;
    un.set_value("NoRepair", &1u32)?;

    if let Some(lnk) = shortcut_path() {
        let _ = create_shortcut(&lnk, &target);
    }
    notify_assoc_changed();
    Ok(())
}

#[cfg(windows)]
pub fn uninstall() {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(classes) = hkcu.open_subkey_with_flags(r"Software\Classes", KEY_ALL_ACCESS) {
        let _ = classes.delete_subkey_all(PROG_ID);
        let _ = classes.delete_subkey_all(r"Applications\mdviewer.exe");
        for ext in EXTS {
            if let Ok(k) =
                classes.open_subkey_with_flags(format!(r"{ext}\OpenWithProgids"), KEY_ALL_ACCESS)
            {
                let _ = k.delete_value(PROG_ID);
            }
        }
    }
    let _ = hkcu.delete_subkey_all(r"Software\mdviewer");
    if let Ok(k) = hkcu.open_subkey_with_flags(r"Software\RegisteredApplications", KEY_ALL_ACCESS) {
        let _ = k.delete_value(APP);
    }
    let _ = hkcu.delete_subkey_all(UNINSTALL_KEY);
    if let Some(lnk) = shortcut_path() {
        let _ = std::fs::remove_file(lnk);
    }
    notify_assoc_changed();

    // O exe em execução não pode se apagar: agenda a remoção da pasta para depois que sair.
    if let Some(dir) = install_dir()
        && dir.exists()
    {
        let _ = hidden_command("cmd")
            .arg("/C")
            .arg(format!(
                "ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"",
                dir.display()
            ))
            .spawn();
    }
    info("mdviewer desinstalado.");
}

#[cfg(windows)]
fn hidden_command(program: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = std::process::Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(windows)]
fn create_shortcut(lnk: &std::path::Path, target: &std::path::Path) -> std::io::Result<()> {
    let esc = |p: &std::path::Path| p.display().to_string().replace('\'', "''");
    let script = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath = '{}'; $s.Save()",
        esc(lnk),
        esc(target)
    );
    hidden_command("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map(|_| ())
}

#[cfg(windows)]
fn open_default_apps_settings() {
    let _ = hidden_command("cmd")
        .args([
            "/C",
            "start",
            "",
            "ms-settings:defaultapps?registeredAppUser=mdviewer",
        ])
        .spawn();
}

#[cfg(windows)]
fn notify_assoc_changed() {
    use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED as i32,
            SHCNF_IDLIST,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
}

#[cfg(not(windows))]
pub fn install() {
    error("A instalação só é suportada no Windows.");
}

#[cfg(not(windows))]
pub fn uninstall() {
    error("A desinstalação só é suportada no Windows.");
}
