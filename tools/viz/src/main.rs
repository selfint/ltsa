use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};

#[derive(Serialize, Deserialize, Debug)]
struct StepPosition {
    line: usize,
    character: usize,
}

#[derive(Serialize, Deserialize, Debug)]
struct Step {
    path: PathBuf,
    start: StepPosition,
    end: StepPosition,
}

#[derive(Serialize, Deserialize, Debug)]
struct Steps {
    steps: Vec<Step>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Stacktraces {
    stacktraces: Vec<Steps>,
}

trait ToHtml {
    fn to_html(&self) -> Result<String>;
}

impl ToHtml for Step {
    fn to_html(&self) -> Result<String> {
        let file_content = String::from_utf8(std::fs::read(&self.path)?)?;
        let mut new_lines = vec![];
        let scroll = 5;
        let start_line = self.start.line - scroll.min(self.start.line);
        let end_line = self.end.line + scroll;
        for (i, line) in file_content.lines().enumerate() {
            if i < start_line || i > end_line {
                continue;
            }

            let mut new_line = format!("{i: >5} | {}", line);

            if i == self.end.line {
                new_line.insert_str(self.end.character + 8, "</mark>")
            }
            if i == self.start.line {
                new_line.insert_str(self.start.character + 8, "<mark>")
            }

            new_lines.push(new_line);
        }

        let new_lines = new_lines.join("\n");
        let style = r#"style="
                        outline-style: solid;
                        outline-color: black;
                        white-space: pre-wrap;
                        ""#;

        Ok(format!(r#"<pre {style}>{new_lines}</pre>"#))
    }
}

impl ToHtml for Steps {
    fn to_html(&self) -> Result<String> {
        let steps_html = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let binding = s.path.clone();
                let path = binding.file_name().unwrap().to_str().unwrap();
                let s = s.to_html()?;
                let path_link = format!(
                    r##"<a href="#" onclick="return show('{path}');">
                        {path}
                    </a>"##
                );

                Ok(format!(
                    r#"<div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>Step: {i} | {path_link}</h2>
                        </div>
                        {s}
                    </div>"#,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(steps_html.join(""))
    }
}

struct Page(Vec<Steps>);

impl ToHtml for Page {
    fn to_html(&self) -> Result<String> {
        let stacktraces_html = self
            .0
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let s = s.to_html()?;
                Ok(format!(
                    r#"<div>
                        <h1>Stacktrace: {i}</h1>
                        {s}
                    </div>"#
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let set_stacktrace_pages = stacktraces_html
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let s = s.replace('`', r#"\`"#);
                format!("pagesMap.set({i}, String.raw`{s}`)")
            })
            .collect::<Vec<_>>()
            .join("\n");

        let stacktrace_links = (0..stacktraces_html.len())
            .map(|i| {
                format!(r##"<a href="#" onclick="return show({i});">Show stacktrace {i}</a>"##)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let step_paths = self
            .0
            .iter()
            .flat_map(|stacktrace| stacktrace.steps.iter().map(|s| s.path.clone()))
            .collect::<HashSet<PathBuf>>();

        let set_file_pages = step_paths
            .iter()
            .map(|p| {
                let content = String::from_utf8(std::fs::read(p)?)?.replace('`', r#"\`"#);
                let filename = p.file_name().unwrap().to_str().unwrap();
                let content_html = format!(
                    r#"
                    <h1>File: {filename}</h1>
                    <pre>{content}</pre>
                "#
                );

                Ok(format!(
                    r#"pagesMap.set("{filename}", String.raw`{content_html}`)"#
                ))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n");

        let file_pages_links = step_paths
            .iter()
            .map(|p| {
                let filename = p.file_name().unwrap().to_str().unwrap();
                format!(
                    r##"<a href="#" onclick="return show('{filename}');">Show file {filename}</a>"##
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            r###"
            <html>
                <head>
                    <script>
                        const pagesMap = new Map();
                        {set_stacktrace_pages}
                        {set_file_pages}

                        function show(index) {{
                            document.querySelector("#content").innerHTML = pagesMap.get(index);
                            return false;
                        }}
                    </script>
                </head>
                <body>
                    <nav>
                        <h2>Pages</h2>
                        {stacktrace_links}
                        |
                        {file_pages_links}
                    </nav>
                    
                    <div id="content">No page selected</div>
                </body>
            </html>
            "###,
        ))
    }
}

fn main() -> Result<()> {
    let steps_path = &std::env::args().nth(1);

    let content = match steps_path.as_deref() {
        None | Some("-") => std::io::read_to_string(std::io::stdin())?,
        Some(steps_path) => String::from_utf8(std::fs::read(steps_path)?)?,
    };

    let stacktraces: Stacktraces = serde_json::from_str(&content)?;

    let page = Page(stacktraces.stacktraces).to_html()?;

    println!("{}", page);

    Ok(())
}
