pub(crate) fn mask_token(token: &str) -> String {
    if token.len() <= 12 {
        return "*".repeat(token.len());
    }
    let head = &token[..6];
    let tail = &token[token.len() - 4..];
    format!("{head}…{tail}")
}

/// Returns the first comma-separated component of a Fly token. For `fm2_*`
/// macaroon bundles, this is the **root** macaroon — Fly rotates the
/// trailing discharges, so an identity-equal token can disagree byte-for-byte
/// with whatever we last snapshotted. The root stays stable for the same
/// principal, making it the right key for "is this the same login?" checks.
pub(crate) fn root_macaroon(token: &str) -> &str {
    token.split(',').next().unwrap_or(token)
}

#[cfg(test)]
mod tests {
    use super::root_macaroon;

    #[test]
    fn root_macaroon_strips_after_first_comma() {
        assert_eq!(root_macaroon("fm2_aaa,fm2_bbb,fo1_ccc"), "fm2_aaa");
    }

    #[test]
    fn root_macaroon_returns_whole_token_when_no_comma() {
        assert_eq!(root_macaroon("fo1_solo"), "fo1_solo");
    }
}

