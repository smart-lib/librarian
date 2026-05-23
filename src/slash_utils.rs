use anyhow::Result;

pub fn split_slash_args(command_line: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut chars = command_line.trim().chars().peekable();
    let mut quote = None;

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some(active), ch) if ch == active => quote = None,
            (Some(_), '\\') => {
                if let Some(next) = chars.next() {
                    current.push(next);
                }
            }
            (Some(_), ch) => current.push(ch),
            (None, '"' | '\'') => quote = Some(ch),
            (None, ch) if ch.is_whitespace() => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            (None, ch) => current.push(ch),
        }
    }

    if quote.is_some() {
        anyhow::bail!("Unclosed quote in slash command");
    }
    if !current.is_empty() {
        args.push(current);
    }
    Ok(args)
}
