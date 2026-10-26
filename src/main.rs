use anyhow::Result;

mod protocol;

const CLASSIFIED_INTELLIGENCE: &str = "secrets.b";

fn main() -> Result<()> {
    protocol::create(CLASSIFIED_INTELLIGENCE)?;
    Ok(())
}
