use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf}; // Added ErrorKind for more specific error handling

// Import Block and related types from crate::block
use crate::block::{Block, HeadLevel, ListOne, ListType, TextSlice};

// Helper function to convert TextSlice vector to a Markdown string
fn format_text_slices_to_markdown(slices: &[TextSlice]) -> String {
    let mut result = String::new();
    for slice in slices {
        let mut current_text = slice.text.clone();
        // Order of application might matter: link should probably wrap styled text.
        // For simplicity now: style, then link.
        if slice.is_code {
            current_text = format!("`{}`", current_text);
        }
        if slice.is_bold {
            // If already code, bold might not render as expected in markdown, but let's keep it simple
            current_text = format!("**{}**", current_text);
        }
        // Markdown doesn't have standard underline. Could use HTML <u> or omit.
        // if slice.is_underline { current_text = format!("<u>{}</u>", current_text); }

        if let Some(link_url) = &slice.link {
            current_text = format!("[{}]({})", current_text, link_url);
        }
        result.push_str(&current_text);
    }
    result
}

// Helper function to convert HeadLevel to a numeric level (usize)
fn head_level_to_usize(level: &HeadLevel) -> usize {
    match level {
        HeadLevel::H1 => 1,
        HeadLevel::H2 => 2,
        HeadLevel::H3 => 3,
        HeadLevel::H4 => 4,
        HeadLevel::H5 => 5,
        HeadLevel::H6 => 6,
        HeadLevel::H7 => 7, // Markdown typically renders H7-H9 as H6 or regular text
        HeadLevel::H8 => 8,
        HeadLevel::H9 => 9,
        HeadLevel::H10 => 10,
    }
}

// Helper function to format list items (can be called recursively for nested lists)
fn format_list_items_to_markdown(
    items: &[ListOne],
    list_type: &ListType,
    indent_level: usize,
    output_md_path: &Path,   // For image paths if lists contain images
    rsc_dir_name: &str,      // For image paths
    image_counter: &mut u32, // Shared image counter
    rsc_path: &Path,         // For copying images
) -> Result<String, Box<dyn std::error::Error>> {
    let mut list_content = String::new();
    let indent = "    ".repeat(indent_level); // 4 spaces for indentation

    for (index, item) in items.iter().enumerate() {
        let headline_md = format_text_slices_to_markdown(&item.headline);
        match list_type {
            ListType::Ordered => {
                list_content.push_str(&format!(
                    "{}{}. {}
",
                    indent,
                    index + 1,
                    headline_md
                ));
            }
            ListType::Unordered => {
                list_content.push_str(&format!(
                    "{}- {}
",
                    indent, headline_md
                ));
            }
            ListType::Task => {
                let checkbox = if item.done.unwrap_or(false) {
                    "[x]"
                } else {
                    "[ ]"
                };
                list_content.push_str(&format!(
                    "{}- {} {}
",
                    indent, checkbox, headline_md
                ));
            }
        }

        // Recursively process following blocks within this list item
        if !item.following.is_empty() {
            let mut nested_block_content = String::new();
            for sub_block in &item.following {
                // Pass image_counter by mutable reference
                nested_block_content.push_str(&process_block_to_markdown(
                    sub_block,
                    output_md_path,
                    rsc_dir_name,
                    image_counter, // Pass as mutable
                    rsc_path,
                    indent_level + 1, // Increase indent for nested blocks
                )?);
            }
            list_content.push_str(&nested_block_content);
        }
    }
    Ok(list_content)
}

// Main processing function for a single block (can be called recursively by lists)
fn process_block_to_markdown(
    block: &Block,
    output_md_path: &Path,   // For context if needed by sub-processors
    rsc_dir_name: &str,      // For image relative paths
    image_counter: &mut u32, // Needs to be mutable
    rsc_path: &Path,         // For copying images
    indent_level: usize,     // For lists
) -> Result<String, Box<dyn std::error::Error>> {
    let mut block_md = String::new();
    let current_indent = "    ".repeat(indent_level);

    match block {
        Block::Text(text_slices) => {
            let formatted_text = format_text_slices_to_markdown(text_slices);
            if !formatted_text.is_empty() {
                // Avoid extra newlines for empty text blocks
                block_md.push_str(&current_indent);
                block_md.push_str(&formatted_text);
                block_md.push_str(
                    "

",
                );
            }
        }
        Block::Title { text, head_level } => {
            let level = head_level_to_usize(head_level).min(6); // Cap at H6 for common markdown
            block_md.push_str(&current_indent);
            block_md.push_str(&format!(
                "{} {}

",
                "#".repeat(level),
                text
            ));
        }
        Block::Image { cached_path } => {
            // Use cached_path as the source, derive alt_text from filename
            let source_image_path = cached_path;
            let image_file_name = source_image_path.file_name().ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("Invalid image cached_path: {:?}", source_image_path),
                )
            })?;

            let alt_text = image_file_name.to_string_lossy().into_owned();

            *image_counter += 1;
            let new_image_file_name =
                format!("{}_{}", *image_counter, image_file_name.to_string_lossy());
            let dest_image_path = rsc_path.join(&new_image_file_name);

            if !source_image_path.exists() {
                return Err(Box::new(io::Error::new(
                    ErrorKind::NotFound,
                    format!("Source image not found: {:?}", source_image_path),
                )));
            }
            fs::copy(source_image_path, &dest_image_path)?;

            let relative_image_path = Path::new(rsc_dir_name).join(new_image_file_name);
            block_md.push_str(&current_indent);
            block_md.push_str(&format!(
                "![{}]({})

",
                alt_text, // Use derived alt_text
                relative_image_path.to_string_lossy().replace("\\", "/")
            ));
        }
        Block::Code { language, code } => {
            block_md.push_str(&current_indent);
            if language.is_empty() {
                block_md.push_str(&format!(
                    "```
{}
```

",
                    code
                ));
            } else {
                block_md.push_str(&format!(
                    "```{}
{}
```

",
                    language, code
                ));
            }
        }
        Block::List { list_type, items } => {
            // initial call for a list block, indent_level passed to format_list_items_to_markdown
            // should ensure the list content itself is not double-indented if process_block_to_markdown adds one.
            // The format_list_items_to_markdown handles indentation for its items.
            let list_md = format_list_items_to_markdown(
                items,
                list_type,
                indent_level, // Pass current indent level for items
                output_md_path,
                rsc_dir_name,
                image_counter, // Pass mutable reference
                rsc_path,
            )?;
            block_md.push_str(&list_md); // format_list_items_to_markdown already adds its own newlines as needed.

            // Add an extra newline after the whole list if it's a top-level block and the list itself is not empty.
            if indent_level == 0 && !list_md.is_empty() {
                // only add extra \n\n if it's not already nested
                block_md.push_str("\n");
            }
        }
    }
    Ok(block_md)
}

pub fn export_blocks_to_markdown(
    blocks: &[Block],
    output_md_path_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Validate output path and get PathBuf
    if !output_md_path_str.ends_with(".md") {
        return Err(Box::new(io::Error::new(
            ErrorKind::InvalidInput,
            "Output path must have a .md extension",
        )));
    }
    let output_md_path = PathBuf::from(output_md_path_str);

    // 2. Determine and create the resource directory (e.g., filename.rsc/)
    let parent_dir = output_md_path.parent().unwrap_or_else(|| Path::new("."));
    let file_stem = output_md_path
        .file_stem()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "Output path has no file stem"))?
        .to_string_lossy();

    let rsc_dir_name = format!("{}.rsc", file_stem);
    let rsc_path = parent_dir.join(&rsc_dir_name);

    fs::create_dir_all(&rsc_path)?;

    // 3. Process blocks and build markdown content
    let mut markdown_content = String::new();
    let mut image_counter: u32 = 0; // Ensure type is explicit if it matters for the helper

    for block in blocks {
        // Use the helper function to process each block
        // The initial indent_level for top-level blocks is 0.
        let block_md = process_block_to_markdown(
            block,
            &output_md_path,    // Pass as reference
            &rsc_dir_name,      // Pass as reference
            &mut image_counter, // Pass as mutable reference
            &rsc_path,          // Pass as reference
            0,                  // Initial indent level for top-level blocks
        )?;
        markdown_content.push_str(&block_md);
    }

    // 4. Write the markdown content to the output file
    fs::write(&output_md_path, markdown_content)?;

    Ok(())
}

// Optional: Add some example usage and tests
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::block::{Block, HeadLevel, ListOne, ListType, TextSlice};
//     use std::fs;
//     use std::io::Read; // For reading file content in tests // Ensure all used types are imported

//     // Helper to create a dummy file for image tests
//     fn create_dummy_file(path: &str) -> io::Result<()> {
//         fs::File::create(path)?;
//         Ok(())
//     }

//     #[test]
//     fn test_export_simple_markdown() -> Result<(), Box<dyn std::error::Error>> {
//         let test_output_dir = Path::new("test_output");
//         fs::create_dir_all(test_output_dir)?;
//         let output_file = test_output_dir.join("simple.md");
//         let output_file_str = output_file.to_str().unwrap();

//         // Define sample blocks using crate::block::Block
//         let blocks = vec![
//             Block::Title {
//                 text: "Main Title".to_string(),
//                 head_level: HeadLevel::H1,
//             },
//             Block::Text(vec![TextSlice {
//                 text: "This is a test paragraph.".to_string(),
//                 is_bold: false,
//                 is_underline: false,
//                 is_code: false,
//                 link: None,
//             }]),
//             Block::Code {
//                 language: "rust".to_string(),
//                 code: "fn main() {
//     println!("Hello");
// }".to_string(),
//             },
//         ];

//         export_blocks_to_markdown(&blocks, output_file_str)?;

//         let mut file = fs::File::open(output_file_str)?;
//         let mut contents = String::new();
//         file.read_to_string(&mut contents)?;

//         assert!(contents.contains("# Main Title"));
//         assert!(contents.contains("This is a test paragraph."));
//         assert!(contents.contains(
//             "```rust
// fn main()"
//         ));

//         // Clean up
//         fs::remove_file(output_file_str)?;
//         fs::remove_dir_all(test_output_dir)?; // Also remove .rsc if created, though not in this test
//         Ok(())
//     }

//     #[test]
//     fn test_export_with_image() -> Result<(), Box<dyn std::error::Error>> {
//         let test_output_dir = PathBuf::from("test_output_img");
//         fs::create_dir_all(&test_output_dir)?;

//         // Create a dummy image file
//         let dummy_image_source_dir = test_output_dir.join("source_images");
//         fs::create_dir_all(&dummy_image_source_dir)?;
//         let dummy_image_path = dummy_image_source_dir.join("test_image.png");
//         create_dummy_file(dummy_image_path.to_str().unwrap())?;

//         let blocks = vec![
//             Block::Title {
//                 text: "Image Test".to_string(),
//                 head_level: HeadLevel::H1,
//             },
//             Block::Image {
//                 cached_path: dummy_image_path.clone(),
//             },
//             Block::Text(vec![TextSlice {
//                 text: "Some text after image.".to_string(),
//                 ..Default::default()
//             }]),
//         ];

//         let output_file = test_output_dir.join("image_doc.md");
//         let output_file_str = output_file.to_str().unwrap();

//         export_blocks_to_markdown(&blocks, output_file_str)?;

//         let mut file = fs::File::open(output_file_str)?;
//         let mut contents = String::new();
//         file.read_to_string(&mut contents)?;

//         assert!(contents.contains("# Image Test"));
//         // Path in markdown should be "image_doc.rsc/1_test_image.png"
//         // Alt text will now be derived from filename: "test_image.png"
//         assert!(contents.contains("![test_image.png](image_doc.rsc/1_test_image.png)"));
//         assert!(contents.contains("Some text after image."));

//         // Check if image was copied
//         let rsc_dir = test_output_dir.join("image_doc.rsc");
//         let copied_image_path = rsc_dir.join("1_test_image.png");
//         assert!(rsc_dir.exists() && rsc_dir.is_dir());
//         assert!(copied_image_path.exists() && copied_image_path.is_file());

//         // Clean up
//         fs::remove_dir_all(test_output_dir)?; // This will remove md file, .rsc dir, and source_images
//         Ok(())
//     }

//     #[test]
//     fn test_invalid_output_path_extension() {
//         let blocks = vec![Block::Text(vec![TextSlice {
//             text: "test".to_string(),
//             ..Default::default()
//         }])];
//         let result = export_blocks_to_markdown(&blocks, "test_output/no_md.txt");
//         assert!(result.is_err());
//         if let Some(err) = result.err() {
//             let error_message = err.to_string();
//             assert!(error_message.contains("Output path must have a .md extension"));
//         }
//     }
//     #[test]
//     fn test_image_not_found() -> Result<(), Box<dyn std::error::Error>> {
//         let test_output_dir = PathBuf::from("test_output_img_not_found");
//         fs::create_dir_all(&test_output_dir)?;

//         let blocks = vec![Block::Image {
//             cached_path: PathBuf::from("non_existent_image.png"),
//         }];

//         let output_file = test_output_dir.join("missing_image_doc.md");
//         let output_file_str = output_file.to_str().unwrap();

//         let result = export_blocks_to_markdown(&blocks, output_file_str);
//         assert!(result.is_err());
//         if let Some(err) = result.err() {
//             // Refine the error checking to be less reliant on exact string matching of quotes
//             // and check the error kind if possible.
//             let io_error_kind = err.downcast_ref::<io::Error>().map(|e| e.kind());
//             assert_eq!(io_error_kind, Some(ErrorKind::NotFound));
//             assert!(err.to_string().contains("Source image not found"));
//             assert!(err.to_string().contains("non_existent_image.png"));
//         }

//         // Clean up
//         fs::remove_dir_all(test_output_dir)?;
//         Ok(())
//     }

//     fn test_export_with_text_slice_formatting() -> Result<(), Box<dyn std::error::Error>> {
//         let test_output_dir = Path::new("test_output_textslice");
//         fs::create_dir_all(test_output_dir)?;
//         let output_file = test_output_dir.join("textslice_doc.md");
//         let output_file_str = output_file.to_str().unwrap();

//         let blocks = vec![Block::Text(vec![
//             TextSlice {
//                 text: "Hello ".to_string(),
//                 ..Default::default()
//             },
//             TextSlice {
//                 text: "bold world".to_string(),
//                 is_bold: true,
//                 ..Default::default()
//             },
//             TextSlice {
//                 text: ", and a ".to_string(),
//                 ..Default::default()
//             },
//             TextSlice {
//                 text: "link".to_string(),
//                 link: Some("http://example.com".to_string()),
//                 ..Default::default()
//             },
//             TextSlice {
//                 text: " with ".to_string(),
//                 ..Default::default()
//             },
//             TextSlice {
//                 text: "code".to_string(),
//                 is_code: true,
//                 ..Default::default()
//             },
//             TextSlice {
//                 text: ".",
//                 ..Default::default()
//             },
//         ])];

//         export_blocks_to_markdown(&blocks, output_file_str)?;

//         let mut file = fs::File::open(output_file_str)?;
//         let mut contents = String::new();
//         file.read_to_string(&mut contents)?;

//         assert!(
//             contents
//                 .contains("Hello **bold world**, and a [link](http://example.com) with `code`.")
//         );

//         fs::remove_dir_all(test_output_dir)?;
//         Ok(())
//     }

//     #[test]
//     fn test_export_with_list() -> Result<(), Box<dyn std::error::Error>> {
//         let test_output_dir = Path::new("test_output_list");
//         fs::create_dir_all(test_output_dir)?;
//         let md_file = test_output_dir.join("list_doc.md");
//         let md_file_str = md_file.to_str().unwrap();

//         // Dummy image for list item
//         let dummy_image_source_dir = test_output_dir.join("source_images_list");
//         fs::create_dir_all(&dummy_image_source_dir)?;
//         let dummy_image_path = dummy_image_source_dir.join("list_item_image.png");
//         create_dummy_file(dummy_image_path.to_str().unwrap())?;

//         let blocks = vec![
//             Block::Title {
//                 text: "Document with Lists".into(),
//                 head_level: HeadLevel::H1,
//             },
//             Block::List {
//                 list_type: ListType::Unordered,
//                 items: vec![
//                     ListOne {
//                         headline: vec![TextSlice {
//                             text: "Item 1".into(),
//                             ..Default::default()
//                         }],
//                         done: None,
//                         following: vec![
//                             Block::Text(vec![TextSlice {
//                                 text: "Sub text for item 1".into(),
//                                 ..Default::default()
//                             }]),
//                             Block::List {
//                                 list_type: ListType::Ordered,
//                                 items: vec![
//                                     ListOne {
//                                         headline: vec![TextSlice {
//                                             text: "Nested ordered 1".into(),
//                                             ..Default::default()
//                                         }],
//                                         done: None,
//                                         following: vec![],
//                                     },
//                                     ListOne {
//                                         headline: vec![TextSlice {
//                                             text: "Nested ordered 2".into(),
//                                             ..Default::default()
//                                         }],
//                                         done: None,
//                                         following: vec![Block::Image {
//                                             cached_path: dummy_image_path.clone(),
//                                         }],
//                                     },
//                                 ],
//                             },
//                         ],
//                     },
//                     ListOne {
//                         headline: vec![TextSlice {
//                             text: "Item 2".into(),
//                             ..Default::default()
//                         }],
//                         done: None,
//                         following: vec![],
//                     },
//                 ],
//             },
//             Block::List {
//                 list_type: ListType::Task,
//                 items: vec![
//                     ListOne {
//                         headline: vec![TextSlice {
//                             text: "Task A (done)".into(),
//                             ..Default::default()
//                         }],
//                         done: Some(true),
//                         following: vec![],
//                     },
//                     ListOne {
//                         headline: vec![TextSlice {
//                             text: "Task B (pending)".into(),
//                             ..Default::default()
//                         }],
//                         done: Some(false),
//                         following: vec![],
//                     },
//                 ],
//             },
//         ];

//         export_blocks_to_markdown(&blocks, md_file_str)?;

//         let mut file = fs::File::open(md_file_str)?;
//         let mut contents = String::new();
//         file.read_to_string(&mut contents)?;
//         println!(
//             "Generated markdown:
// {}",
//             contents
//         ); // For debugging test

//         assert!(contents.contains(
//             "- Item 1
// "
//         ));
//         assert!(contents.contains(
//             "    Sub text for item 1
// "
//         ));
//         assert!(contents.contains(
//             "    1. Nested ordered 1
// "
//         ));
//         assert!(contents.contains(
//             "    2. Nested ordered 2
// "
//         ));
//         assert!(
//             contents.contains("    ![list_item_image.png](list_doc.rsc/1_list_item_image.png)")
//         ); // Image counter starts from 1 for this export
//         assert!(contents.contains(
//             "- Item 2
// "
//         ));
//         assert!(contents.contains(
//             "- [x] Task A (done)
// "
//         ));
//         assert!(contents.contains(
//             "- [ ] Task B (pending)
// "
//         ));

//         // Check copied image
//         let rsc_dir = test_output_dir.join("list_doc.rsc");
//         let copied_image = rsc_dir.join("1_list_item_image.png"); // Check based on counter logic
//         assert!(
//             copied_image.exists(),
//             "Image {:?} should have been copied",
//             copied_image
//         );

//         fs::remove_dir_all(test_output_dir)?;
//         Ok(())
//     }
// }
