use std::path::PathBuf;

use thirtyfour::{By, WebElement};

use crate::{BlockId, webelement_ext::WebElementExt};

#[derive(Debug)]
pub struct TextSlice {
    text: String,
    is_bold: bool,
    is_underline: bool,
    is_code: bool,
}

#[derive(Debug)]
pub struct ListOne {
    headline: Vec<TextSlice>,
    following: Vec<Block>,
}

#[derive(Debug)]
pub enum HeadLevel {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    H7,
    H8,
    H9,
    H10,
}

impl HeadLevel {
    /// e is heading-block, whose child contains heading-h{x} class
    async fn get_for_heading_block(e: &WebElement) -> Option<HeadLevel> {
        if e.get_direct_children(".heading-h1").await.len() > 0 {
            Some(HeadLevel::H1)
        } else if e.get_direct_children(".heading-h2").await.len() > 0 {
            Some(HeadLevel::H2)
        } else if e.get_direct_children(".heading-h3").await.len() > 0 {
            Some(HeadLevel::H3)
        } else if e.get_direct_children(".heading-h4").await.len() > 0 {
            Some(HeadLevel::H4)
        } else if e.get_direct_children(".heading-h5").await.len() > 0 {
            Some(HeadLevel::H5)
        } else if e.get_direct_children(".heading-h6").await.len() > 0 {
            Some(HeadLevel::H6)
        } else if e.get_direct_children(".heading-h7").await.len() > 0 {
            Some(HeadLevel::H7)
        } else if e.get_direct_children(".heading-h8").await.len() > 0 {
            Some(HeadLevel::H8)
        } else if e.get_direct_children(".heading-h9").await.len() > 0 {
            Some(HeadLevel::H9)
        } else if e.get_direct_children(".heading-h10").await.len() > 0 {
            Some(HeadLevel::H10)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum Block {
    /// markdown format
    Text(Vec<TextSlice>),
    Title {
        text: String,
        head_level: HeadLevel,
    },
    List {
        ordered: bool,
        items: Vec<ListOne>,
    },
    Image {
        cached_path: PathBuf,
    },
    Code {
        language: String,
        code: String,
    },
}

impl Block {
    pub async fn new_by_element(e: &WebElement) -> Self {
        // head case
        {
            let child = e.find_all(By::Css(":scope > .heading-block")).await;
            if let Ok(child) = child {
                if let Some(child) = child.get(0) {
                    // is head
                    let head_level = HeadLevel::get_for_heading_block(&child).await.unwrap();
                    // get text
                    let prefix = if let Ok(order) = child
                        .find_all(By::Css(":scope > .heading > .heading-order"))
                        .await
                    {
                        if let Some(order) = order.get(0) {
                            Some(order.text().await.unwrap())
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    let content = child
                        .find_all(By::Css(":scope > .heading > .heading-content"))
                        .await
                        .unwrap()[0]
                        .text()
                        .await
                        .unwrap();

                    let ret = Block::Title {
                        text: content,
                        head_level,
                    };

                    println!("extracted heading: {:?}", ret);
                    return ret;
                }
            }
        }

        //

        Block::Text(vec![])
    }
}
