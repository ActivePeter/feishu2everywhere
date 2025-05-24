use std::path::PathBuf;

use thirtyfour::{By, WebElement};

use crate::{BlockId, webelement_ext::WebElementExt};

#[derive(Debug, Default)]
pub struct TextSlice {
    text: String,
    is_bold: bool,
    is_underline: bool,
    is_code: bool,
    link: Option<String>,
}

// #[derive(Debug, Default)]
// pub struct Link {
//     url: String,
//     shown_name: String,
// }

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

async fn try_new_heading(e: &WebElement) -> Option<Block> {
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
            return Some(ret);
        }
    }
    None
}

// class ace-line contains text-slices
async fn get_text_slices_for_are_line(e: &WebElement) -> Vec<TextSlice> {
    let mut text_slices = vec![];
    let children_spans = e.get_direct_children("span").await;

    for children_span in children_spans {
        if let Ok(children_link) = children_span.find_all(By::Css(".link")).await {
            if let Some(children_link) = children_link.get(0) {
                let text = children_link.text().await.unwrap();
                text_slices.push(TextSlice {
                    text,
                    is_bold: false,
                    is_underline: false,
                    is_code: false,
                    link: Some(children_link.get_attribute("href").await.unwrap().unwrap()),
                });
                continue;
            }
        }

        // inline-code
        if let Ok(children_code) = children_span.find_all(By::Css(".inline-code")).await {
            if let Some(children_code) = children_code.get(0) {
                let text = children_code.text().await.unwrap();
                text_slices.push(TextSlice {
                    text,
                    is_code: true,
                    ..Default::default()
                });
                continue;
            }
        }

        // bold
        let style = children_span.get_attribute("style").await.unwrap();
        if let Some(style) = style {
            // trim all spaces
            let style = style.trim();
            if style.contains("font-weight:bold;") {
                text_slices.push(TextSlice {
                    text: children_span.text().await.unwrap(),
                    is_bold: true,
                    ..Default::default()
                });
                continue;
            }
        }

        // todo
        // - underline

        // treat as common

        text_slices.push(TextSlice {
            text: children_span.text().await.unwrap(),
            ..Default::default()
        });
    }

    text_slices
}

async fn try_new_text(e: &WebElement) -> Option<Block> {
    // if let Some(child) = e.get_direct_children(".text-block-wrapper").await.get(0) {
    //     let mut text_slices = vec![];
    // }

    // :scope > .text-block-wrapper > .text-block > .zone-container > .ace-line
    if let Some(child) = e
        .get_direct_children(".text-block-wrapper > .text-block > .zone-container > .ace-line")
        .await
        .get(0)
    {
        println!("extracted text");
        let ret = Block::Text(get_text_slices_for_are_line(&child).await);

        println!("extracted text: {:?}", ret);

        return Some(ret);
    }

    None
}

async fn try_new_code(e: &WebElement) -> Option<Block> {
    // :scope
    // > .docx-code-block-container
    // > .docx-code-block-inner-container
    // > .code-block-resize
    // > .resizable-wrapper
    // > .code-block
    // > .code-block-content

    // 代码语言(暂不支持，飞书鼠标不悬停会隐藏对应内容)

    // 代码内容
    // > .text-editor (子元素是若干行ace-line)
    let code_elem = e.get_direct_children(".docx-code-block-container").await;
    let Some(code_elem) = code_elem.get(0) else {
        return None;
    };

    let Ok(code_elem) = code_elem
        .find_all(By::Css(".code-block-content > .zone-container"))
        .await
    else {
        return None;
    };

    let Some(code_elem) = code_elem.get(0) else {
        return None;
    };
    // let select = String::new()
    //     + "> .docx-code-block-container "
    //     + "> .docx-code-block-inner-container "
    //     + "> .code-block-resize "
    //     + "> .resizable-wrapper "
    //     + "> .code-block "
    //     + "> .code-block-content";

    // if let Some(child) = e.get_direct_children(&select).await.get(0) {
    let mut code = String::new();
    for line in code_elem.get_direct_children(".ace-line").await {
        code.push_str(&line.text().await.unwrap());
        code.push('\n');
    }

    let ret = Some(Block::Code {
        language: "".to_string(),
        code,
    });

    println!("extracted code: {:?}", ret);
    return ret;

    None
}

impl Block {
    pub async fn new_by_element(e: &WebElement) -> Self {
        // head case
        if let Some(block) = try_new_heading(e).await {
            return block;
        }

        // text case
        if let Some(block) = try_new_text(e).await {
            return block;
        }

        // code case
        if let Some(block) = try_new_code(e).await {
            return block;
        }

        Block::Text(vec![])
    }
}
