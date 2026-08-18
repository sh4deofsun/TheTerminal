//! ZDOTDIR override: inject OSC 133/7 hooks without touching the user's rc files.

use std::path::PathBuf;

const ZSHENV: &str = r#"
_TT_ZDOTDIR="$ZDOTDIR"
_TT_USER="${TERMAPP_USER_ZDOTDIR:-$HOME}"
if [[ -f "$_TT_USER/.zshenv" ]]; then ZDOTDIR="$_TT_USER" builtin source "$_TT_USER/.zshenv"; fi
ZDOTDIR="$_TT_ZDOTDIR"
"#;

const ZPROFILE: &str = r#"
[[ -f "$_TT_USER/.zprofile" ]] && ZDOTDIR="$_TT_USER" builtin source "$_TT_USER/.zprofile"
"#;

/// `{}` is the managed alias file, sourced after the user's rc so an alias
/// edited in the settings UI wins over the rc definition.
const ZSHRC: &str = r#"
[[ -f "$_TT_USER/.zshrc" ]] && ZDOTDIR="$_TT_USER" builtin source "$_TT_USER/.zshrc"

[[ -f "{}" ]] && builtin source "{}"

autoload -Uz add-zsh-hook
__tt_precmd() {
  local __tt_ec=$?
  if (( ${__TT_EXEC:-0} )); then printf '\e]133;D;%s\a' "$__tt_ec"; fi
  __TT_EXEC=0
  printf '\e]7;file://%s%s\a' "${HOST}" "${PWD}"
  printf '\e]133;A\a'
  printf '\e]133;B\a'
}
__tt_preexec() { __TT_EXEC=1; printf '\e]133;C\a'; }
add-zsh-hook precmd __tt_precmd
add-zsh-hook preexec __tt_preexec

# Restore for nested shells: they load the user's config directly (no marks →
# the session falls back to raw single-block mode, by design).
if [[ "$_TT_USER" != "$HOME" ]]; then ZDOTDIR="$_TT_USER"; else unset ZDOTDIR; fi
"#;

const ZLOGIN: &str = r#"
[[ -f "$_TT_USER/.zlogin" ]] && ZDOTDIR="$_TT_USER" builtin source "$_TT_USER/.zlogin"
"#;

/// Writes the override dir, returns its path.
pub fn setup() -> std::io::Result<PathBuf> {
    let dir = std::env::temp_dir().join("theterminal-zdot");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(".zshenv"), ZSHENV)?;
    std::fs::write(dir.join(".zprofile"), ZPROFILE)?;
    let aliases = crate::alias::file().unwrap_or_default();
    let aliases = aliases.display().to_string();
    std::fs::write(dir.join(".zshrc"), ZSHRC.replace("{}", &aliases))?;
    std::fs::write(dir.join(".zlogin"), ZLOGIN)?;
    Ok(dir)
}
