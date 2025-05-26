use std::path::PathBuf;

use base64;
use sha2::{Digest, Sha256};
use thirtyfour::{By, WebDriver, WebElement};

use crate::{BlockId, webelement_ext::WebElementExt};

const IMAGE_CACHE_DIR: &str = "image_cache";

#[derive(Debug, Default, Clone)]
pub struct TextSlice {
    pub text: String,
    pub is_bold: bool,
    pub is_underline: bool,
    pub is_code: bool,
    pub link: Option<String>,
}

// #[derive(Debug, Default)]
// pub struct Link {
//     url: String,
//     shown_name: String,
// }

#[derive(Debug, Clone)]
pub struct ListOne {
    pub done: Option<bool>,
    pub headline: Vec<TextSlice>,
    pub following: Vec<Block>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListType {
    Ordered,
    Unordered,
    Task,
}

#[derive(Debug, Clone)]
pub enum Block {
    /// markdown format
    Text(Vec<TextSlice>),
    Title {
        text: String,
        head_level: HeadLevel,
    },
    List {
        list_type: ListType,
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
}

#[derive(Debug)]
pub enum OneOf<A, B> {
    A(A),
    B(B),
}

// Make B generic for Clone as well
impl<A: Clone, B_ITEM: Clone> Clone for OneOf<A, B_ITEM> {
    fn clone(&self) -> Self {
        match self {
            OneOf::A(a) => OneOf::A(a.clone()),
            OneOf::B(b) => OneOf::B(b.clone()),
        }
    }
}

async fn try_new_todo_list(e: &WebElement) -> Option<OneOf<Block, (ListType, ListOne)>> {
    // the we get todo state by one of the 2 case
    // .todo-block && .task-done (first try this)
    // .todo-block

    // Get all todo block elements
    let todo_elems = e.get_direct_children(".todo-block").await;

    // If no todo elements found, return None
    if todo_elems.is_empty() {
        return None;
    }

    // Check for a single todo item case
    if todo_elems.len() == 1 {
        let todo_elem = &todo_elems[0];

        // Check if the todo item is done by looking at its class name
        let is_done = match todo_elem.get_attribute("class").await {
            Ok(Some(class_name)) => class_name.contains("task-done"),
            _ => false,
        };

        let content_elems = todo_elem.find_all(By::Css(".ace-line")).await.unwrap();
        if !content_elems.is_empty() {
            let headline = get_text_slices_for_are_line(&content_elems[0]).await;
            let following = vec![]; // For now, not handling nested blocks

            let ret = ListOne {
                done: Some(is_done),
                headline,
                following,
            };

            println!("extracted todo list: {:?}", ret);
            // Return a single ListOne item, now with ListType
            return Some(OneOf::B((ListType::Task, ret)));
        } else {
            panic!("todo block should have content");
        }
    } else {
        panic!("one todo list block should have only one item");
    }
}

async fn try_new_common_list(e: &WebElement) -> Option<OneOf<Block, (ListType, ListOne)>> {
    // get unordered by .bullet-list > .list
    // get ordered by .ordered-list > .list

    // Try to find ordered list elements
    let ordered_elems = e.get_direct_children(".ordered-list > .list").await;
    let is_ordered = !ordered_elems.is_empty();

    // Try to find unordered list elements
    let unordered_elems = e.get_direct_children(".bullet-list > .list").await;

    // Determine which list type we found
    let (list_elem, determined_list_type) = if is_ordered {
        (&ordered_elems[0], ListType::Ordered)
    } else if !unordered_elems.is_empty() {
        (&unordered_elems[0], ListType::Unordered)
    } else {
        println!("no list found");
        return None;
    };

    // Get the content for the list item
    let ace_lines = list_elem.find_all(By::Css(".ace-line")).await.unwrap();
    assert!(!ace_lines.is_empty());
    // if ace_lines.is_empty() {
    //     return None;
    // }

    // Extract text for the first line
    let headline = get_text_slices_for_are_line(&ace_lines[0]).await;
    let following = vec![]; // For now, not handling nested blocks

    let ret = ListOne {
        done: None, // Not a todo list
        headline,
        following,
    };

    println!("extracted common list item: {:?}", ret);
    return Some(OneOf::B((determined_list_type, ret)));
}

/// we only prepare the head of list,
/// the following items will be processed when all Blocks are collected
/// and will be contructed by pre-known dependency of elements
async fn try_new_list(e: &WebElement) -> Option<OneOf<Block, (ListType, ListOne)>> {
    // First try to extract todo list
    if let Some(result) = try_new_todo_list(e).await {
        // Already has the correct return type, just pass it through
        return Some(result);
    }

    // Then try to extract common list (ordered or unordered)
    if let Some(result) = try_new_common_list(e).await {
        // Already has the correct return type, just pass it through
        return Some(result);
    }

    None
}

async fn try_new_image(driver: &WebDriver, ctx_str: &str, e: &WebElement) -> Option<Block> {
    // direct: .block-comment > .docx-block-loading-container
    let container = e
        .get_direct_children(".block-comment > .docx-block-loading-container")
        .await;
    if container.is_empty() {
        return None;
    }

    // find_all: canvas
    let canvas_result = e.find_all(By::Css("canvas")).await;
    if canvas_result.is_err() || canvas_result.as_ref().unwrap().is_empty() {
        return None;
    }

    let canvas = &canvas_result.unwrap()[0];

    //  # get the canvas as a PNG base64 string
    //  canvas_base64 = driver.execute_script("return arguments[0].toDataURL('image/png').substring(21);", canvas)
    //  # decode
    //  canvas_png = base64.b64decode(canvas_base64)

    let canvas_base64 = unsafe {
        driver
            .execute(
                "return arguments[0].toDataURL('image/png').substring(22);",
                vec![canvas.to_json().unwrap()],
            )
            .await
            .unwrap()
    };
    let canvas_base64 = canvas_base64.json().as_str().unwrap();
    println!("canvas_base64: {}", canvas_base64);
    let canvas_png = base64::decode(canvas_base64).unwrap();

    // Create a hash from the image data for a unique filename
    let mut hasher = Sha256::new();
    hasher.update(&canvas_png);
    let hash = format!("{:x}", unsafe { hasher.finalize() });

    // save to {IMAGE_CACHE_DIR}/{ctx_str}/{SUMMARY_HASH}.png
    let cache_dir = std::path::Path::new(IMAGE_CACHE_DIR).join(ctx_str);
    if !cache_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            println!("Failed to create directory: {:?}", e);
            return None;
        }
    }

    let image_path = cache_dir.join(format!("{}.png", &hash[0..16]));
    if let Err(e) = std::fs::write(&image_path, canvas_png) {
        println!("Failed to write image: {:?}", e);
        return None;
    }

    println!("Saved image to: {:?}", image_path);

    Some(Block::Image {
        cached_path: image_path,
    })
}

impl Block {
    pub async fn new_by_element(
        driver: &WebDriver,
        ctx_str: &str,
        e: &WebElement,
    ) -> Option<OneOf<Block, (ListType, ListOne)>> {
        // head case
        if let Some(block) = try_new_heading(e).await {
            return Some(OneOf::A(block));
        }

        // text case
        if let Some(block) = try_new_text(e).await {
            return Some(OneOf::A(block));
        }

        // code case
        if let Some(block) = try_new_code(e).await {
            return Some(OneOf::A(block));
        }

        // image case
        if let Some(block) = try_new_image(driver, ctx_str, e).await {
            return Some(OneOf::A(block));
        }

        // list case (todo list, ordered list, unordered list)
        if let Some(result) = try_new_list(e).await {
            return Some(result);
        }

        None
    }
}

impl ListOne {
    pub fn new(headline: Vec<TextSlice>, done: Option<bool>, following: Vec<Block>) -> Self {
        Self {
            headline,
            done,
            following,
        }
    }

    pub fn get_headline(&self) -> &Vec<TextSlice> {
        &self.headline
    }

    pub fn get_done(&self) -> &Option<bool> {
        &self.done
    }

    pub fn get_following(&self) -> &Vec<Block> {
        &self.following
    }

    pub fn get_following_mut(&mut self) -> &mut Vec<Block> {
        &mut self.following
    }
}
