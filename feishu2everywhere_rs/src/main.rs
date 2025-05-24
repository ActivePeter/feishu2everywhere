mod block;
mod log;
mod poll_keys;
mod webelement_ext;

use std::cell::RefCell;
use std::cmp::{Ord, Ordering as CmpOrdering, PartialOrd};
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::Write;
use std::process::Stdio;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_recursion::async_recursion;
use base64::{Engine as _, engine::general_purpose};
use block::Block;
use device_query::{DeviceQuery, DeviceState, Keycode};
use log::LogType;
use thirtyfour::{By, DesiredCapabilities, WebDriver, WebElement};
use tokio;
use tokio::process::{Child, Command};

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

/// return blockid -> (webelement, children ids)
async fn collect_blocks(
    running: &AtomicBool,
    driver: &WebDriver,
) -> HashMap<BlockId, (WebElement, Vec<BlockId>)> {
    // let mut last_id = None;
    let mut all_skip_times = 0;
    let mut collected_blocks = HashMap::new();
    let mut appeared_id = HashSet::new();

    // Initialize element map
    let mut element_map = find_enabled_element(&driver).await;

    while running.load(Ordering::SeqCst) && !element_map.is_empty() {
        let mut skip_times = 0;
        let initial_map_size = element_map.len();

        // Process elements in map, one at a time to avoid reference issues
        while !element_map.is_empty() {
            // Get the first key (smallest ID)
            let id = *element_map.keys().next().unwrap();

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
            Block::new_by_element(&e).await;

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

            println!("elem {} contains children: {:?}", id, child_elem_ids);
            tokio::time::sleep(Duration::from_millis(1000)).await;

            collected_blocks.insert(id, (e, child_elem_ids));
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

    println!("doc is all dump");
    collected_blocks
}

#[tokio::main]
async fn main() {
    kill_old_chrome().await;

    let mut child = run_chromedriver();

    // Set up WebDriver
    let mut caps = DesiredCapabilities::chrome();
    caps.add_chrome_arg("--disk-cache-size=0").unwrap();
    caps.add_chrome_arg("--media-cache-size=0").unwrap();
    caps.add_chrome_arg("--disable-gpu-shader-disk-cache")
        .unwrap();
    caps.add_chrome_arg("--user-data-dir=./user").unwrap();
    // caps.set_binary("../prepare/prepare_cache/chromedriver")
    //     .unwrap();

    let driver = WebDriver::new("http://localhost:9518", caps).await.unwrap();

    // Navigate to the Feishu document
    driver
        .goto("https://fvd360f8oos.feishu.cn/docx/Q3c6dJG5Go3ov6xXofZcGp43nfb")
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

    let block_elems = collect_blocks(&running, &driver).await;

    // for a elem, child of whose child should be removed from its children

    // print each block children
    let mut max_id = 0;
    for (id, (elem, child_ids)) in block_elems.iter() {
        println!("block {} has children: {:?}", id, child_ids);
        if *id > max_id {
            max_id = *id;
        }
    }

    for id in 0..max_id {
        if !block_elems.contains_key(&id) {
            println!("block {} not found", id);
        }
    }

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

    // wait for ctrl+c
    tokio::signal::ctrl_c().await.unwrap();

    // Close the driver
    driver.quit().await.unwrap();

    child.kill().await.unwrap();
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
