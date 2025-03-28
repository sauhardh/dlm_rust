use url;
use url::Url;

/// Validate the url before processing.
pub fn validate_url(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    Url::parse(url)?;
    Ok(())
}

/// Removes '.' '/' '\' ':'  from the url string
pub fn filter_name(name: String) -> String {
    let name_iter: Vec<&str> = name
        .split("")
        .filter(|x| ![".", "/", "\\", ":"].contains(x))
        .collect();

    name_iter.concat().trim().to_string()
}
