use super::error::SshError;

pub fn cwd_hook(nonce: &str) -> Result<String, SshError> {
    if nonce.len() != 8 || !nonce.chars().all(|char| char.is_ascii_hexdigit()) {
        return Err(SshError::new("invalid_input"));
    }
    let command = r#"printf '\033]7;file://localhost%s\007' "$PWD"; [ -n "$BASH_VERSION" ] && PROMPT_COMMAND=${PROMPT_COMMAND:+$PROMPT_COMMAND; }'printf "\033]7;file://localhost%s\007" "$PWD"'; [ -n "$ZSH_VERSION" ] && eval 'precmd_pixo() { printf "\033]7;file://localhost%s\007" "$PWD"; }; precmd_functions+=(precmd_pixo)'"#;
    Ok(format!("{command}; {color}; stty echo # PIXO{nonce}\n", color = color_setup()))
}

fn color_setup() -> &'static str {
    r#"[ -n "$BASH_VERSION" ] && case "$PS1" in *$'\e'*|*$'\033'*) ;; *) PS1='\[\e[1;32m\]\u\[\e[0m\]@\[\e[1;36m\]\h\[\e[0m\]:\[\e[1;34m\]\w\[\e[0m\]\$ ';; esac; [ -n "$ZSH_VERSION" ] && case "$PS1" in *$'\e'*|*%F*) ;; *) PS1='%F{green}%n@%m%f:%F{blue}%~%f%# ';; esac; command -v dircolors >/dev/null 2>&1 && eval "$(dircolors -b 2>/dev/null)"; if ls --color=auto -d . >/dev/null 2>&1; then alias ls='ls --color=auto'; elif ls -G -d . >/dev/null 2>&1; then alias ls='ls -G'; fi"#
}

pub fn shell_single_quote(value: &str) -> Result<String, SshError> {
    if value.is_empty()
        || value.len() > 512
        || value
            .chars()
            .any(|char| char == '\0' || char == '\n' || char == '\r')
    {
        return Err(SshError::new("invalid_input"));
    }
    Ok(format!("'{}'", value.replace('\'', "'\\''")))
}

#[cfg(test)]
mod tests {
    use super::shell_single_quote;

    #[test]
    fn quotes_spaces_and_embedded_quotes() {
        assert_eq!(shell_single_quote("/tmp/a b").unwrap(), "'/tmp/a b'");
        assert_eq!(shell_single_quote("o'brien").unwrap(), "'o'\\''brien'");
    }

    #[test]
    fn rejects_newlines() {
        assert!(shell_single_quote("a\nb").is_err());
        assert!(shell_single_quote("").is_err());
    }

    #[test]
    fn cwd_hook_is_one_hidden_line() {
        let command = super::cwd_hook("ab12cd34").unwrap();
        assert_eq!(command.matches('\n').count(), 1);
        assert!(command.ends_with("; stty echo # PIXOab12cd34\n"));
        assert!(command.contains("$BASH_VERSION"));
        assert!(command.contains("$ZSH_VERSION"));
        assert!(command.contains("PS1="));
        assert!(command.contains("ls --color=auto"));
        assert!(super::cwd_hook("nope").is_err());
    }
}
