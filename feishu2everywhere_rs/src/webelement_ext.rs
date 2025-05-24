use thirtyfour::{By, WebElement};

pub trait WebElementExt {
    async fn get_direct_children(&self, child_css: &str) -> Vec<WebElement>;
}

impl WebElementExt for WebElement {
    async fn get_direct_children(&self, child_css: &str) -> Vec<WebElement> {
        match self
            .find_all(By::Css(&format!(":scope > {child_css}")))
            .await
        {
            Err(err) => {
                vec![]
            }
            Ok(children) => children,
        }
    }
}
