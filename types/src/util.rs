pub fn quote_str(s: &str) -> String {
    //todo: really works the same as the Go version?
    format!("\"{:?}\"", s)
}

pub fn short_quote_str(s: &str, max: usize) -> String {
    let result = format!("\"{:?}\"", s);
    shorten_with_ellipsis(result, max)
}

pub fn shorten_with_ellipsis(s: String, max: usize) -> String {
    if s.len() <= max {
        s
    } else {
        let mut buf: Vec<char> = s.chars().collect();
        buf = buf[0..(buf.len() - 3)].to_vec();
        buf.append(&mut "...".to_owned().chars().collect());
        buf.into_iter().collect()
    }
}

#[cfg(test)]
mod test {
    //use super::*;
}
