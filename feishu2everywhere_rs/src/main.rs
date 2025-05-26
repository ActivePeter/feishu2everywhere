mod block;
mod log;
mod poll_keys;
mod to_markdown;
mod webelement_ext;

use std::borrow::BorrowMut;
use std::cell::RefCell;
use std::cmp::{Ord, Ordering as CmpOrdering, PartialOrd};
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::ops::Mul;
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_recursion::async_recursion;
use base64::{Engine as _, engine::general_purpose};
use block::{Block, ListOne, ListType, OneOf};
use device_query::{DeviceQuery, DeviceState, Keycode};
use log::LogType;
use thirtyfour::{By, DesiredCapabilities, WebDriver, WebElement};
use tokio;
use tokio::process::{Child, Command};

#[tokio::main]
async fn main() {
    let config = Config {
        headless: false,
        output_md: "out.md".to_string(),
    };

    kill_old_chrome().await;

    let mut child = run_chromedriver();

    // Set up WebDriver
    let mut caps = DesiredCapabilities::chrome();
    caps.add_chrome_arg("--disk-cache-size=0").unwrap();
    caps.add_chrome_arg("--media-cache-size=0").unwrap();
    caps.add_chrome_arg("--disable-gpu-shader-disk-cache")
        .unwrap();
    caps.add_chrome_arg("--user-data-dir=./user").unwrap();
    if config.headless {
        caps.add_chrome_arg("--headless").unwrap();
    }
    // caps.set_binary("../prepare/prepare_cache/chromedriver")
    //     .unwrap();

    let driver = WebDriver::new("http://localhost:9518", caps).await.unwrap();

    // Navigate to the Feishu document
    driver
        .goto("https://qcnoe3hd7k5c.feishu.cn/wiki/FSe2wulLqiHcI8kgCqIcmmlknnh")
        .await
        .unwrap();

    // Wait for page to load
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Create output file
    let mut outmd = File::create("out.md").unwrap();

    // Track processed elements and images
    let mut appear: HashMap<String, bool> = HashMap::new();
    let mut appear_img: HashMap<String, i32> = HashMap::new();

    // Setup Ctrl+C detection
    let running = Arc::new(AtomicBool::new(true));

    poll_keys::start_poll_keys(running.clone());

    let final_blocks = collect_blocks(&running, &driver).await;

    // for a elem, child of whose child should be removed from its children

    // print each block children
    // let mut max_id = 0;
    // for (id, (elem, child_ids)) in block_elems.iter() {
    //     println!("block {} has children: {:?}", id, child_ids);
    //     if *id > max_id {
    //         max_id = *id;
    //     }
    // }

    // for id in 0..max_id {
    //     if !block_elems.contains_key(&id) {
    //         println!("block {} not found", id);
    //     }
    // }

    // print max_id

    // .into_iter()
    // .map(|v| RefCell::new(v))
    // .collect();

    // // scan blocks, if block has the child block, add them to child list
    // for elem_id_elem in block_elems.iter() {
    //     let elem_id = elem_id_elem.borrow().0;

    //     // elem > list-wrapper > list-children > render-unit-wrapper > block

    // }

    // loop {
    //     tokio::time::sleep(Duration::from_secs(1000)).await;
    // }

    to_markdown::export_blocks_to_markdown(
        &final_blocks.into_values().collect::<Vec<_>>(),
        &config.output_md,
    )
    .unwrap();

    // wait for ctrl+c
    tokio::signal::ctrl_c().await.unwrap();

    // Close the driver
    driver.quit().await.unwrap();

    child.kill().await.unwrap();
}

/// A wrapper around WebElement that includes its block ID for ordering

struct WebElementWithId {
    id: BlockId,
    element: WebElement,
}

impl Eq for WebElementWithId {}

impl PartialEq for WebElementWithId {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialOrd for WebElementWithId {
    fn partial_cmp(&self, other: &Self) -> Option<CmpOrdering> {
        Some(self.cmp(other))
    }
}

impl Ord for WebElementWithId {
    fn cmp(&self, other: &Self) -> CmpOrdering {
        // Reverse order for BinaryHeap (max heap) to behave like a min heap
        other.id.cmp(&self.id)
    }
}

/// Finds elements and returns them as a BTreeMap ordered by block ID
/// New elements with the same ID will replace older ones
async fn find_enabled_element(driver: &WebDriver) -> BTreeMap<BlockId, WebElement> {
    // collect all elements with following css selector
    //  root-render-unit-container > .render-unit-wrapper > .block
    let elements: Vec<WebElement> = driver
        .find_all(By::Css(
            ".root-render-unit-container > .render-unit-wrapper > .block",
        ))
        .await
        .unwrap();

    let mut element_map = BTreeMap::new();

    for element in elements {
        if let Ok(Some(id_str)) = element.get_attribute("data-block-id").await {
            if let Ok(id) = id_str.parse::<i32>() {
                // Newer elements with the same ID will replace older ones
                element_map.insert(id, element);
            }
        }
    }

    element_map
}

/// Add WebElements to the BTreeMap
async fn add_elements_to_map(elements: Vec<WebElement>, map: &mut BTreeMap<BlockId, WebElement>) {
    for element in elements {
        if let Ok(Some(id_str)) = element.get_attribute("data-block-id").await {
            if let Ok(id) = id_str.parse::<i32>() {
                map.insert(id, element);
            } else {
                println!("Failed to parse block ID: {}", id_str);
            }
        }
    }
}

async fn find_child_elements(driver: &WebDriver, element: &WebElement) -> Vec<WebElement> {
    // .root-render-unit-container > .render-unit-wrapper > .block

    let elements: Vec<WebElement> = element
        .find_all(By::Css(
            ".root-render-unit-container > .render-unit-wrapper > .block",
        ))
        .await
        .unwrap();

    elements
}

pub type BlockId = i32;

// Define InternalBlockPart structure at the module level
#[derive(Debug)]
struct InternalBlockPart {
    content: OneOf<Block, (ListType, ListOne)>,
    children: Vec<BlockId>,
}

/// return blockid -> (webelement, children ids)
async fn collect_blocks(running: &AtomicBool, driver: &WebDriver) -> BTreeMap<BlockId, Block> {
    // let mut last_id = None;
    let mut all_skip_times = 0;
    let mut collected_blocks = HashMap::new();
    let mut appeared_id = HashSet::new();

    // Initialize element map
    let mut element_map = find_enabled_element(&driver).await;

    // Define InternalBlockPart structure
    let mut blockid_2_block_or_listone: BTreeMap<BlockId, InternalBlockPart> = BTreeMap::new();

    while running.load(Ordering::SeqCst) && !element_map.is_empty() {
        let mut skip_times = 0;
        let initial_map_size = element_map.len();

        // Process elements in map, one at a time to avoid reference issues
        while !element_map.is_empty() {
            // Get the first key (smallest ID)
            let id = *element_map.keys().next().unwrap();
            // wait for element to be ready
            tokio::time::sleep(Duration::from_millis(1000)).await;

            // Get and remove the element from the map
            let e = element_map.remove(&id).unwrap();

            if let Err(err) = e.scroll_into_view().await {
                println!("err scroll_into_view: {:?}", err);
                continue;
            }

            // refetch blocks and update element_map
            {
                let new_element_map = find_enabled_element(&driver).await;
                for (elem_id, elem) in new_element_map {
                    if elem_id > id && !element_map.contains_key(&elem_id) {
                        element_map.insert(elem_id, elem);
                    }
                }
            }

            if appeared_id.contains(&id) {
                println!("skip appeared id: {}", id);
                skip_times += 1;
                continue;
            } else {
                appeared_id.insert(id);
            }

            println!("\n=============one element=============");
            println!("id: {}", id);
            println!("text: {}", e.text().await.unwrap());
            let blockpart = Block::new_by_element(&driver, "one", &e).await;

            if blockpart.is_none() {
                println!("unrecognized element");
                // continue;
            };

            let child_elem_ids = {
                let child_elems = e.find_all(By::Css(".block")).await;

                if let Ok(child_elems) = child_elems {
                    let mut child_elem_ids = vec![];
                    let mut child_element_map: BTreeMap<BlockId, WebElement> = BTreeMap::new();

                    // Process children and add to temporary map
                    for elem in child_elems.iter() {
                        if let Ok(Some(id_str)) = elem.get_attribute("data-block-id").await {
                            if let Ok(child_id) = id_str.parse::<i32>() {
                                child_elem_ids.push(child_id);
                            }
                        }
                    }

                    // Add child elements to the main map for processing
                    add_elements_to_map(child_elems.clone(), &mut element_map).await;

                    for elem in child_elems {
                        // find child of child
                        let child_of_child_elems = elem.find_elements(By::Css(".block")).await;
                        if let Ok(child_of_child_elems) = child_of_child_elems {
                            // Also add these deeper child elements to the map
                            add_elements_to_map(child_of_child_elems.clone(), &mut element_map)
                                .await;

                            for child_of_child_elem in child_of_child_elems.iter() {
                                if let Ok(Some(id_str)) =
                                    child_of_child_elem.get_attribute("data-block-id").await
                                {
                                    if let Ok(child_of_child_id) = id_str.parse::<i32>() {
                                        // remove child_of_child_elem_id from child_elem_ids
                                        child_elem_ids.retain(|v| *v != child_of_child_id);
                                    }
                                }
                            }
                        }
                    }

                    child_elem_ids
                } else {
                    vec![]
                }
            };

            if let Some(blockpart) = blockpart {
                blockid_2_block_or_listone.insert(
                    id,
                    InternalBlockPart {
                        content: blockpart,
                        children: child_elem_ids.clone(),
                    },
                );

                println!("elem {} contains children: {:?}", id, child_elem_ids);

                collected_blocks.insert(id, (e, child_elem_ids));
            }
        }

        // If no new elements were processed in this cycle
        if skip_times == initial_map_size {
            all_skip_times += 1;
        } else {
            all_skip_times = 0;
        }

        if all_skip_times > 3 {
            break;
        } else if all_skip_times > 0 {
            println!("elements all skip");
        }

        // If map is empty, fetch more elements
        if element_map.is_empty() {
            element_map = find_enabled_element(&driver).await;
            println!("continue collect elements");
        }
    }

    // all scaned to blockid_2_block_or_listone
    // for each blockid_2_block_or_listone, construct real block
    // - we use a root_list option to record list in root
    // - we use a ctx stack, each with (parent id, unmatched children) to record recent unfilled parent(children are not all inserted)
    // - for one block or diff type listone, we always first try take the root_list and add it before handling current
    // - for one block, if it's not in ctx children, common just add to vec,
    // - for listone, add to or create root_list
    // - for one block or list one, if it's in ctx's children, remove it in ctx unmatched children and add to parent sub (parent is supposed to be a Block::List)
    let final_blocks = construct_blocks(blockid_2_block_or_listone);

    println!("final_blocks:");
    fn debug_block(block: &Block, depth: usize) {
        match block {
            Block::List { list_type, items } => {
                println!("{}list {:?}", " ".repeat(depth), list_type);
                for item in items {
                    println!("{}- {:?}", " ".repeat(depth), item.get_headline());
                    for child in item.get_following() {
                        debug_block(child, depth + 2);
                    }
                    println!("");
                }
            }
            block => {
                println!("{}{:?}\n", " ".repeat(depth), block);
            }
        }
    }
    // fn dfs
    for (id, block) in &final_blocks {
        debug_block(block, 0);
    }

    println!("doc is all dump");
    final_blocks
}

struct Config {
    headless: bool,
    output_md: String,
}

async fn kill_old_chrome() {
    if cfg!(target_os = "windows") {
        Command::new("taskkill")
            .args(&["/f", "/im", "chrome.exe"])
            .output()
            .await
            .expect("Failed to kill chrome process");
    } else {
        Command::new("killall")
            .args(&["-9", "Google Chrome for Testing"])
            .output()
            .await
            .expect("Failed to kill chrome process");
    }
}

//
fn run_chromedriver() -> Child {
    // use chrono to get current time
    let file = log::new_log_file(LogType::ChromeDriver);

    // realtime output to file
    let mut child = Command::new("../prepare/prepare_cache/chromedriver")
        .args(&["--port=9518"])
        .stdout(Stdio::from(file))
        .spawn()
        .unwrap();

    // child.wait().unwrap();

    // Command::new("chromedriver")
    //     .args(&["--port=9515"])
    //     .output()
    //     .expect("Failed to run chromedriver");
    child
}

async fn scroll(
    driver: &WebDriver,
    container: &thirtyfour::WebElement,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("begin scroll");
    driver
        .action_chain()
        .move_by_offset(0, 250)
        .perform()
        .await?;
    println!("end scroll");
    // unsafe {
    //     driver
    //         .execute(
    //             r#"
    //     arguments[0].scrollBy({
    //         top: 250,
    //         behavior: 'smooth'
    //     });
    //     "#,
    //             vec![container.to_json()?],
    //         )
    //         .await?;
    // }
    Ok(())
}

// fn depth_pre(depth: usize) -> String {
//     match depth {
//         0 => "".to_string(),
//         1 => "   ".to_string(),
//         2 => "      ".to_string(),
//         3 => "         ".to_string(),
//         4 => "            ".to_string(),
//         5 => "               ".to_string(),
//         _ => "                  ".to_string(),
//     }
// }

// async fn text_detail(
//     element: &thirtyfour::WebElement,
// ) -> Result<String, Box<dyn std::error::Error>> {
//     let mut text = String::new();

//     match element.find_element(By::Css(".ace-line")).await {
//         Ok(line) => {
//             let line_spans = line.find_elements(By::Css(":scope > span")).await?;

//             for span in line_spans {
//                 // Try to find mention-doc
//                 match span.find_element(By::Css(".mention-doc")).await {
//                     Ok(ref_elem) => {
//                         let href = ref_elem.get_attribute("href").await?.unwrap_or_default();
//                         let alias = ref_elem.text().await?;
//                         text.push_str(&format!("[{}]({})", alias, href));
//                     }
//                     Err(_) => {
//                         // Try to find link
//                         match span.find_element(By::Css(".link")).await {
//                             Ok(ref_elem) => {
//                                 let href =
//                                     ref_elem.get_attribute("href").await?.unwrap_or_default();
//                                 let alias = ref_elem.text().await?;
//                                 text.push_str(&format!("[{}]({})", alias, href));
//                             }
//                             Err(_) => {
//                                 // Try to find inline-code
//                                 match span.find_element(By::Css(".inline-code")).await {
//                                     Ok(ref_elem) => {
//                                         let code_text = ref_elem.text().await?;
//                                         text.push_str(&format!("`{}`", code_text));
//                                     }
//                                     Err(_) => {
//                                         // Check if bold
//                                         let font_weight = span.css_value("font-weight").await?;
//                                         if font_weight == "bold" {
//                                             text.push_str(&format!("**{}**", span.text().await?));
//                                         } else {
//                                             text.push_str(&span.text().await?);
//                                         }
//                                     }
//                                 }
//                             }
//                         }
//                     }
//                 }
//             }
//         }
//         Err(_) => {
//             text = "not line text".to_string();
//         }
//     }

//     Ok(text)
// }

// async fn write_with_depth(
//     text: &str,
//     depth: usize,
//     outmd: &mut File,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     let prefix = depth_pre(depth);
//     let mut w = text.replace("\n", &format!("\n{}", prefix));
//     w = format!("{}{}", prefix, w);

//     if w.ends_with(" ") {
//         w = w[0..w.len() - 1].to_string() + "\n";
//     }

//     println!("{} {}", depth, w);
//     outmd.write_all(w.as_bytes())?;

//     Ok(())
// }

// #[async_recursion]
// async fn append(
//     element: &thirtyfour::WebElement,
//     depth: usize,
//     appear: &mut HashMap<String, bool>,
//     appear_img: &mut HashMap<String, i32>,
//     outmd: &mut File,
// ) -> Result<bool, Box<dyn std::error::Error>> {
//     let eclass = element.class_name().await?.unwrap_or_default();
//     let textfmt = text_detail(element).await?;
//     let element_text = element.text().await?.trim().to_string();
//     let ordertext = element_text.replacen("\n", " ", 1);
//     let id = element
//         .get_attribute("data-record-id")
//         .await?
//         .unwrap_or_default();

//     let mut succ = true;

//     if eclass.contains("docx-heading1-block") {
//         write_with_depth(&format!("# {}\n\n", textfmt), depth, outmd).await?;
//     } else if eclass.contains("docx-heading2-block") {
//         write_with_depth(&format!("## {}\n\n", textfmt), depth, outmd).await?;
//     } else if eclass.contains("docx-text-block") {
//         write_with_depth(&format!("{}\n\n", textfmt), depth, outmd).await?;
//     } else if eclass.contains("docx-code-block") {
//         write_with_depth(&format!("```\n{}\n```\n\n", element_text), depth, outmd).await?;
//     } else if eclass.contains("docx-ordered-block") {
//         append_list(element, depth, appear, appear_img, outmd).await?;
//     } else if eclass.contains("docx-unordered-block") {
//         write_with_depth(&format!("{}\n\n", ordertext), depth, outmd).await?;
//     } else if eclass.contains("docx-todo-block") {
//         write_with_depth(&format!("- {}\n\n", element_text.trim()), depth, outmd).await?;
//     } else if eclass.contains("docx-whiteboard-block")
//         || eclass.contains("docx-synced_source-block")
//     {
//         succ = false;
//         // Try to find canvas
//         match element.find_element(By::Css("canvas")).await {
//             Ok(canvas) => {
//                 let canvas_base64 = unsafe {
//                     // Get canvas as PNG base64 string
//                     element
//                         .handle
//                         .execute(
//                             "return arguments[0].toDataURL('image/png').substring(21);",
//                             vec![canvas.to_json()?],
//                         )
//                         .await?
//                         .value()
//                         .to_string()
//                 };

//                 // Decode base64
//                 let canvas_png = general_purpose::STANDARD.decode(&canvas_base64)?;

//                 let img_filename = format!("canvas{}.png", id);
//                 fs::write(&img_filename, canvas_png)?;

//                 if !appear_img.contains_key(&id) {
//                     write_with_depth(&format!("![canvas]({})\n\n", img_filename), depth, outmd)
//                         .await?;
//                     appear_img.insert(id.clone(), 1);
//                 }
//             }
//             Err(_) => println!("canvas not found"),
//         }
//     } else {
//         write_with_depth(
//             &format!("{}:{}\n\n", eclass, element_text.trim()),
//             depth,
//             outmd,
//         )
//         .await?;
//     }

//     if succ {
//         appear.insert(id, true);
//     }

//     Ok(succ)
// }

// async fn append_list(
//     listblock: &thirtyfour::WebElement,
//     depth: usize,
//     appear: &mut HashMap<String, bool>,
//     appear_img: &mut HashMap<String, i32>,
//     outmd: &mut File,
// ) -> Result<(), Box<dyn std::error::Error>> {
//     let list = listblock
//         .find_element(By::Css(".list-wrapper > .list"))
//         .await?;
//     let listtext = text_detail(&list).await?;

//     outmd.write_all(format!("{}{}\n\n", depth_pre(depth), listtext).as_bytes())?;

//     // Try to find children
//     match listblock
//         .find_element(By::Css(".list-wrapper > .list-children"))
//         .await
//     {
//         Ok(list_children) => {
//             let child_elems = list_children
//                 .find_elements(By::Css(":scope > .render-unit-wrapper > .block"))
//                 .await?;

//             for e in child_elems {
//                 append(&e, depth + 1, appear, appear_img, outmd).await?;
//             }
//         }
//         Err(_) => {}
//     }

//     Ok(())
// }

// async fn collect_elements(
//     driver: &WebDriver,
//     appear: &mut HashMap<String, bool>,
//     appear_img: &mut HashMap<String, i32>,
//     outmd: &mut File,
// ) -> Result<i32, Box<dyn std::error::Error>> {
//     let root_css = ".root-render-unit-container > .render-unit-wrapper > .block";
//     let elements = driver.find_elements(By::Css(root_css)).await?;

//     let mut newcnt = 0;

//     for e in elements {
//         let block_id = e.get_attribute("data-record-id").await?.unwrap_or_default();
//         let eclass = e.class_name().await?.unwrap_or_default();

//         if eclass.contains("docx-whiteboard-block")
//             || eclass.contains("docx-synced_source-block")
//             || !appear.contains_key(&block_id)
//         {
//             if append(&e, 0, appear, appear_img, outmd).await? {
//                 newcnt += 1;
//             }
//         }
//     }

//     println!("collect_elements {}", newcnt);
//     Ok(newcnt)
// }

/// Process blocks and construct hierarchical structure
/// 将构建过程分为三个阶段:
/// 1. 记录父子关系阶段：记录每个元素的父亲Some(id)，没有父亲就是None
/// 2. 倒序构建阶段：
///    - 如果是listone且父亲item尾巴是同类listone，就加入该listone的following
///    - 如果是listone且父亲item尾巴不是同类，就new一个Block::list
/// 3. 反转following阶段：把每个listone的following都reverse，因为是倒序加入的
fn construct_blocks(
    blockid_2_block_or_listone: BTreeMap<BlockId, InternalBlockPart>,
) -> BTreeMap<BlockId, Block> {
    // 将输入转换为可变的结构
    let mut mutable_blocks: BTreeMap<BlockId, RefCell<Option<InternalBlockPart>>> = BTreeMap::new();

    // 直接移动原始数据到可变结构
    for (id, block_part) in blockid_2_block_or_listone {
        mutable_blocks.insert(id, RefCell::new(Some(block_part)));
    }

    // 第一阶段：记录父子关系
    let parent_map = {
        let mut parent_map: BTreeMap<BlockId, Option<BlockId>> = BTreeMap::new();

        // 初始化所有块的父节点为None
        for (block_id, _) in &mutable_blocks {
            parent_map.insert(*block_id, None);
        }

        // 遍历记录父子关系
        for (block_id, block_part_cell) in &mutable_blocks {
            if let Some(block_part) = &*block_part_cell.borrow() {
                for child_id in &block_part.children {
                    if parent_map.contains_key(child_id) {
                        parent_map.insert(*child_id, Some(*block_id));
                        println!("Set block {} parent to {}", child_id, block_id);
                    }
                }
            }
        }

        // 打印父子关系供调试
        println!("Parent relationships:");
        for (id, parent_id) in &parent_map {
            println!("Block {} has parent: {:?}", id, parent_id);
        }
        parent_map
    };

    // 第二阶段：倒序构建
    for (id, block_part_cell) in mutable_blocks.iter().rev() {
        if let Some(parent) = parent_map.get(id).unwrap() {
            let take_cur_block = block_part_cell.borrow_mut().take().unwrap();
            // 如果parent是listone，且block_part_cell是同类listone，就加入该listone的following
            let mut parent_block = mutable_blocks.get(parent).unwrap().borrow_mut();
            let parent_block = parent_block.as_mut().unwrap();

            let parent_listone = match &mut parent_block.content {
                OneOf::B((_, listone)) => listone,
                _ => panic!("parent is not listone-like"),
            };

            match take_cur_block.content {
                OneOf::A(block) => {
                    parent_listone.get_following_mut().push(block);
                }
                OneOf::B((child_list_type, child_list_one)) => {
                    fn push_new_list_to_parent_following(
                        parent_listone: &mut ListOne,
                        cl_type: ListType,
                        cl_one: ListOne,
                    ) {
                        unsafe {
                            parent_listone.get_following_mut().push(Block::List {
                                list_type: cl_type,
                                items: vec![cl_one],
                            });
                        }
                    }

                    if let Some(last_block_in_parent_following) =
                        parent_listone.get_following_mut().iter_mut().last()
                    {
                        match last_block_in_parent_following {
                            Block::List {
                                list_type: parent_inner_list_type,
                                items: parent_inner_items,
                            } if *parent_inner_list_type == child_list_type => {
                                parent_inner_items.push(child_list_one);
                            }
                            _ => {
                                push_new_list_to_parent_following(
                                    parent_listone,
                                    child_list_type,
                                    child_list_one,
                                );
                            }
                        }
                    } else {
                        push_new_list_to_parent_following(
                            parent_listone,
                            child_list_type,
                            child_list_one,
                        );
                    }
                }
            };

            // listone
            //     .get_following_mut()
            //     .push();
        }
    }

    let mut result_blocks = mutable_blocks
        .into_iter()
        .filter_map(|(id, block_cell)| {
            if let Some(internal_block_part) = block_cell.borrow_mut().take() {
                match internal_block_part.content {
                    OneOf::A(block_content) => Some((id, block_content)),
                    OneOf::B((actual_list_type, list_one_instance)) => Some((id, unsafe {
                        Block::List {
                            list_type: actual_list_type,
                            items: vec![list_one_instance],
                        }
                    })),
                }
            } else {
                None
            }
        })
        .collect::<BTreeMap<_, _>>();

    // 第三阶段：反转following
    // 由于我们直接修改了原始结构，需要反转所有ListOne的following
    {
        fn reverse_recursive(block: &mut Block) {
            if let Block::List { items, .. } = block {
                // Reverse the order of items within the current list first
                items.reverse();
                // Then, for each item (which is a ListOne), recursively reverse its 'following' blocks
                for item in items {
                    for following_block in item.get_following_mut() {
                        reverse_recursive(following_block);
                    }
                }
            }
        }
        for (_, block) in &mut result_blocks {
            reverse_recursive(block);
        }
    }

    // Stage 4: Group consecutive root lists of the same type (User's method)
    // Part 1: Identify groups and populate to_group
    // to_group stores (target_block_id, Vec<source_block_ids_to_merge_and_remove>)
    let mut to_group: Vec<(BlockId, Vec<BlockId>)> = Vec::new();

    let mut current_group_target_id: Option<BlockId> = None;
    let mut current_group_list_type: Option<ListType> = None;
    let mut current_group_sources: Vec<BlockId> = Vec::new();

    // BTreeMap iterates in key-sorted order, which is what we need for "consecutive"
    for (id, block) in result_blocks.iter() {
        if let Block::List { list_type, .. } = block {
            if current_group_target_id.is_some() && current_group_list_type == Some(*list_type) {
                // This block is part of the currently tracked group
                current_group_sources.push(*id);
            } else {
                // This block starts a new group or is a different type of list.
                // Finalize the previous group if it had sources.
                if let Some(target_id) = current_group_target_id {
                    if !current_group_sources.is_empty() {
                        to_group.push((target_id, current_group_sources.clone()));
                    }
                }
                // Start a new group with the current block as the target.
                current_group_target_id = Some(*id);
                current_group_list_type = Some(*list_type);
                current_group_sources.clear();
            }
        } else {
            // Current block is not a list. Finalize any open list group.
            if let Some(target_id) = current_group_target_id {
                if !current_group_sources.is_empty() {
                    to_group.push((target_id, current_group_sources.clone()));
                }
            }
            // Reset group tracking
            current_group_target_id = None;
            current_group_list_type = None;
            current_group_sources.clear();
        }
    }
    // After the loop, finalize the last tracked group if it exists and has sources.
    if let Some(target_id) = current_group_target_id {
        if !current_group_sources.is_empty() {
            to_group.push((target_id, current_group_sources.clone()));
        }
    }

    // Part 2: Merge based on to_group, modifying result_blocks
    for (target_id, source_ids) in to_group {
        // We need to get the items from source_ids first, then modify target_id,
        // to avoid mutable borrow issues if target_id itself is a source_id (should not happen with this logic).
        let mut items_to_add_to_target: Vec<ListOne> = Vec::new();

        for source_id in &source_ids {
            if let Some(removed_block) = result_blocks.remove(source_id) {
                if let Block::List { items, .. } = removed_block {
                    items_to_add_to_target.extend(items);
                } else {
                    panic!("source_id is not a list: {}", source_id);
                }
            } else {
                panic!("source_id not found in result_blocks: {}", source_id);
            }
        }

        if !items_to_add_to_target.is_empty() {
            if let Some(target_block) = result_blocks.get_mut(&target_id) {
                if let Block::List {
                    items: target_items,
                    ..
                } = target_block
                {
                    target_items.extend(items_to_add_to_target);
                } else {
                    panic!("target_id is not a list: {}", target_id);
                }
            } else {
                panic!("target_id not found in result_blocks: {}", target_id);
            }
        }
    }

    result_blocks
}
