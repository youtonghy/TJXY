pub(crate) fn split_title_year(value: &str) -> Option<(&str, i32)> {
    let trimmed = value.trim();
    let prefix = trimmed.strip_suffix(')')?;
    let (name, year) = prefix.rsplit_once('(')?;
    if year.len() != 4 || name.trim().is_empty() {
        return None;
    }
    let year = year.parse::<i32>().ok()?;
    (1..=9999).contains(&year).then_some((name.trim(), year))
}

#[cfg(test)]
mod tests {
    use super::split_title_year;

    #[test]
    fn accepts_compact_and_spaced_names() {
        for value in ["Arrival(2016)", "Arrival (2016)", "Arrival   (2016)"] {
            assert_eq!(split_title_year(value), Some(("Arrival", 2016)));
        }
    }

    #[test]
    fn rejects_non_year_suffixes_and_empty_titles() {
        for value in ["Formula (1)", "Arrival(20A6)", "Arrival (10000)", " (2016)"] {
            assert_eq!(split_title_year(value), None);
        }
    }
}
