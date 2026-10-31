use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

/// Runs a program and returns its trimmed stdout, failing on a non-zero exit.
pub fn output(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("failed to start `{program}` (is it installed?)"))?;
    if !out.status.success() {
        bail!(
            "`{program}` failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Puts text on the Wayland clipboard.
pub fn copy(text: &str) -> Result<()> {
    copy_as(None, text)
}

/// Copies `text` with a MIME type, e.g. `text/uri-list` so file managers paste the file itself.
pub fn copy_as(mime: Option<&str>, text: &str) -> Result<()> {
    let mut cmd = Command::new("wl-copy");
    if let Some(mime) = mime {
        cmd.args(["--type", mime]);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to start `wl-copy` (install wl-clipboard)")?;
    child.stdin.take().unwrap().write_all(text.as_bytes())?;
    child.wait()?;
    Ok(())
}

/// Shows a desktop notification; silently does nothing if notify-send is missing.
pub fn notify(title: &str, body: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "quiver", "-t", "2500", title, body])
        .status();
}

/// Quotes text as a Lua string literal, for Hyprland's Lua dispatchers.
pub fn lua_str(text: &str) -> String {
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Quotes text for `sh -c`, e.g. it's → 'it'\''s'.
pub fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Runs Lua dispatcher expressions like `hl.dsp.window.pin({...})` in one hyprctl call.
/// Hyprland 0.55+ replaced the old `hyprctl dispatch exec foo` syntax with Lua.
pub fn dispatch(dispatchers: &[String]) -> Result<()> {
    let code: Vec<String> = dispatchers.iter().map(|d| format!("hl.dispatch({d})")).collect();
    let reply = output("hyprctl", &["eval", &code.join("\n")])?;
    if reply.starts_with("error") {
        bail!("Hyprland rejected the command: {reply}");
    }
    Ok(())
}

/// Asks Hyprland to launch a command, so it isn't a child of quiver.
pub fn spawn(cmd: &str) -> Result<()> {
    dispatch(&[format!("hl.dsp.exec_cmd({})", lua_str(cmd))])
}

/// The user's terminal: $TERMINAL, else the first of foot/kitty/alacritty installed.
pub fn terminal() -> String {
    if let Ok(term) = std::env::var("TERMINAL") {
        return term;
    }
    ["foot", "kitty", "alacritty"]
        .into_iter()
        .find(|t| output("which", &[t]).is_ok())
        .unwrap_or("foot")
        .to_string()
}

/// Runs a shell command in a terminal that stays open so you can read the output.
pub fn spawn_in_terminal(cmd: &str) -> Result<()> {
    spawn(&format!("{} --hold sh -c {}", terminal(), sh_quote(cmd)))
}

/// The JSON reply of a `hyprctl -j <query>` call, e.g. "clients" or "monitors all".
pub fn hypr_query(query: &str) -> Result<serde_json::Value> {
    let mut args = vec!["-j"];
    args.extend(query.split_whitespace());
    Ok(serde_json::from_str(&output("hyprctl", &args)?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_lua_and_shell() {
        assert_eq!(lua_str(r#"say "hi" \ bye"#), r#""say \"hi\" \\ bye""#);
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
    }
}
