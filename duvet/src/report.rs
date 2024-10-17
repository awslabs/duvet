use duvet_core::Result;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Report {
    pub manifest_path: PathBuf,
}

impl Report {
    pub async fn run(&self) -> Result<()> {
        let manifest = crate::manifest::load(self.manifest_path.clone().into()).await?;
        let (comments, errors) = crate::comment::query_group(manifest.clone()).await;

        let rfc8999 = duvet_core::http::get_cached_string(
            "https://www.rfc-editor.org/rfc/rfc8999.txt",
            "target/www.rfc-editor.org/rfc/rfc8999.txt",
        )
        .await?;
        let tokens: Vec<_> = crate::ietf::tokenizer::tokens(&rfc8999).collect();
        dbg!(tokens);

        dbg!(comments);
        dbg!(errors);
        Ok(())
    }
}
