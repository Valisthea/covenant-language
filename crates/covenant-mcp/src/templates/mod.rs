/// Return a rendered template for the given construct keyword, with `{{NAME}}`
/// replaced by `name`. Returns `None` for unknown construct strings.
pub fn render(construct: &str, name: &str) -> Option<String> {
    let tpl = template_for(construct)?;
    Some(tpl.replace("{{NAME}}", name))
}

fn template_for(construct: &str) -> Option<&'static str> {
    Some(match construct {
        "record" => include_str!("record.cov"),
        "token" => include_str!("token.cov"),
        "confidential token" | "confidentialtoken" => include_str!("confidential_token.cov"),
        "ballot" => include_str!("ballot.cov"),
        "counter" => include_str!("counter.cov"),
        "encrypted counter" | "encryptedcounter" => include_str!("encrypted_counter.cov"),
        "board" => include_str!("board.cov"),
        "market" => include_str!("market.cov"),
        "vault" => include_str!("vault.cov"),
        "registry" => include_str!("registry.cov"),
        "bridge" => include_str!("bridge.cov"),
        "ceremony" => include_str!("ceremony.cov"),
        "module" => include_str!("module.cov"),
        "hybrid module" | "hybridmodule" => include_str!("hybrid_module.cov"),
        _ => return None,
    })
}
