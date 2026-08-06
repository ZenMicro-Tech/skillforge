use anyhow::{bail, Context, Result};
use std::io::{self, BufRead, Write};

use crate::credentials;
use crate::oci;

const DEFAULT_REGISTRY: &str = "ghcr.io";

pub fn login(registry: Option<&str>, username: Option<&str>, password_stdin: bool) -> Result<()> {
    let registry = registry.unwrap_or(DEFAULT_REGISTRY);

    let username = match username {
        Some(u) => u.to_string(),
        None => prompt_line(&format!("Username for {registry}: "))?,
    };
    if username.trim().is_empty() {
        bail!("username must not be empty");
    }

    let secret = if password_stdin {
        let mut s = String::new();
        io::stdin()
            .lock()
            .read_line(&mut s)
            .context("reading password from stdin")?;
        s.trim_end_matches(['\n', '\r']).to_string()
    } else {
        rpassword::prompt_password(format!("Password / token for {registry}: "))
            .context("reading password")?
    };
    if secret.is_empty() {
        bail!("password/token must not be empty");
    }

    eprintln!("verifying credentials against {registry}...");
    if let Err(e) = oci::verify_login(registry, &username, &secret) {
        bail!("login failed: {e}");
    }

    credentials::set(registry, &username, &secret)
        .with_context(|| format!("saving credentials for {registry}"))?;

    eprintln!(
        "✓ logged in to {registry} as {username} (saved to {})",
        credentials::path().display()
    );
    Ok(())
}

pub fn logout(registry: Option<&str>) -> Result<()> {
    let registry = registry.unwrap_or(DEFAULT_REGISTRY);
    if credentials::remove(registry)? {
        eprintln!("✓ removed stored credentials for {registry}");
    } else {
        eprintln!("no stored credentials for {registry}");
    }
    Ok(())
}

fn prompt_line(prompt: &str) -> Result<String> {
    eprint!("{prompt}");
    io::stderr().flush().ok();
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .context("reading input")?;
    Ok(line.trim_end_matches(['\n', '\r']).to_string())
}
