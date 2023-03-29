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

            let mut new_line = line.to_string();

            if i == self.end.line {
                new_line.insert_str(self.end.character, "</mark>")
            }
            if i == self.start.line {
                new_line.insert_str(self.start.character, "<mark>")
            }

            let new_line = format!("<span>{new_line}</span>");

            new_lines.push(new_line);
        }

        let new_lines = new_lines.join("\n");
        let style = format!(
            r#"style="
                white-space: pre-wrap;
                counter-set: line {start_line}
                ""#
        );

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
                    r##"<a href="#" onclick="return setFile('{path}');">
                        {path}
                    </a>"##
                );

                Ok(format!(
                    r#"<div style="outline-style: solid; outline-color: black; padding: 0.3rem; margin-bottom: 1rem">
                        <div>
                            <h2>{: >3}. {path_link}</h2>
                        </div>
                        {s}
                    </div>"#,
                    i + 1
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
                let i = i + 1;
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
                let i = i + 1;
                let s = s.replace('`', r#"\`"#);
                format!("pagesMap.set({i}, String.raw`{s}`)")
            })
            .collect::<Vec<_>>()
            .join("\n");

        let stacktrace_links = (0..stacktraces_html.len())
            .map(|i| {
                let i = i + 1;
                format!(
                    r##"<a href="#" onclick="return setStacktrace({i});">Show stacktrace {i}</a>"##
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let step_paths = self
            .0
            .iter()
            .flat_map(|stacktrace| stacktrace.steps.iter().map(|s| s.path.clone()))
            .collect::<HashSet<PathBuf>>();

        let mut step_paths = step_paths.iter().collect::<Vec<_>>();

        step_paths.sort();

        let set_file_pages = step_paths
            .iter()
            .map(|p| {
                let content = String::from_utf8(std::fs::read(p)?)?
                    .replace('`', r#"\`"#)
                    .lines()
                    .map(|l| format!("<span>{l}</span>"))
                    .collect::<Vec<_>>()
                    .join("\n");

                let filename = p.file_name().unwrap().to_str().unwrap();
                let content_html = format!(
                    r#"
                    <h1>File: {filename}</h1>
                    <pre style="white-space: pre-wrap";>{content}</pre>
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
                    r##"<a href="#" onclick="return setFile('{filename}');">Show file {filename}</a>"##
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let style = r#"
            pre {
                // background: #303030;
                // color: #f1f1f1;
                // padding: 10px 16px;
                // border-radius: 2px;
                // border-top: 4px solid #00aeef;
                // -moz-box-shadow: inset 0 0 10px #000;
                // box-shadow: inset 0 0 10px #000;
            }
            
            pre span {
                counter-increment: line;
            }
            pre span:before {
                content: counter(line);
                display: inline-block;
                border-right: 1px solid #ddd;
                padding: 0 .5em;
                margin-right: .5em;
                color: #888;
                width: 5ch;
                text-align: right;
            }

            #content {
                display: grid;
                grid-template-areas: 'stacktrace file';
                grid-area-columns: 1fr 1fr;
            }

            #stacktrace {
                grid-area: "stacktrace";
            }

            #file {
                grid-area: "file";
            }
            "#;

        Ok(format!(
            r###"
            <!DOCTYPE html>
            <html>
                <head>
                    <style>{style}</style>
                    <script>
                        const pagesMap = new Map();
                        {set_stacktrace_pages}
                        {set_file_pages}

                        function setStacktrace(index) {{
                            document.querySelector("#stacktrace").innerHTML = pagesMap.get(index);
                            document.querySelector("#stacktrace").style.display = "block";
                            return false;
                        }}

                        function setFile(index) {{
                            document.querySelector("#file").innerHTML = pagesMap.get(index);
                            document.querySelector("#file").style.display = "block";
                            return false;
                        }}

                        function hideStacktrace(index) {{
                            document.querySelector("#stacktrace").style.display = "none";
                            return false;
                        }}

                        function hideFile(index) {{
                            document.querySelector("#file").style.display = "none";
                            return false;
                        }}
                    </script>
                </head>
                <body>
                    <nav style="background: #b5b5b5;">
                        <h2>Pages</h2>
                        {stacktrace_links}
                        <a href="#" onclick="return hideStacktrace();">Hide stacktrace</a>
                        <br />---<br />
                        {file_pages_links}
                        <a href="#" onclick="return hideFile();">Hide file</a>
                    </nav>
                    
                    <div id="content">
                        <div id="stacktrace">No stacktrace selected</div>
                        <div id="file">No file selected</div>
                    </div>
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
