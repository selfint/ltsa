use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

            new_lines.push(new_line);
        }

        let new_lines = new_lines.join("\n");

        Ok(format!(
            r#"<pre style="outline-style: solid; outline-color: black; white-space: pre-wrap">{new_lines}</pre>"#
        ))
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

                Ok(format!(
                    r#"<div style="outline-style: solid; outline-color: black">
                        <h2><b>Step: {i}</b></h2>
                        <h3><b>Path: </b>{path}</h2>
                        {s}
                    </div>"#,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(steps_html.join(""))
    }
}

impl ToHtml for Stacktraces {
    fn to_html(&self) -> Result<String> {
        let stacktraces_html = self
            .stacktraces
            .iter()
            .map(|s| s.to_html())
            .collect::<Result<Vec<_>>>()?;

        let stacktraces = stacktraces_html
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    r#"<div style="outline-style: solid; outline-color: black"><h1><b>Stacktrace: {i}</b></h1>{s}</div>"#,
                )
            })
            .collect::<Vec<_>>();

        Ok(stacktraces.join(""))
    }
}

struct Page(Stacktraces);

impl ToHtml for Page {
    fn to_html(&self) -> Result<String> {
        let stacktraces = self
            .0
            .stacktraces
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let s = s.to_html()?;
                Ok(format!(
                    r#"<div style="outline-style: solid; outline-color: black"><h1><b>Stacktrace: {i}</b></h1>{s}</div>"#,
                ))
            })
            .collect::<Result<Vec<_>>>()?;

        let set_pages_map = stacktraces
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let s = s.replace('`', r#"\`"#);
                format!("pagesMap.set({i}, String.raw`{s}`)")
            })
            .collect::<Vec<_>>()
            .join("\n");

        let links = (0..stacktraces.len())
            .map(|i| {
                format!(r##"<a href="#" onclick="return show({i});">Show stacktrace {i}</a>"##)
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            r###"
            <html>
                <body>
                    <nav>
                        {links}
                    </nav>
        
                    <div id="content">tesfdsafdsaf</div>
                    
                    <script>
                        const pagesMap = new Map();
                        {set_pages_map}

                        function show(index) {{
                            document.querySelector("#content").innerHTML = pagesMap.get(index);
                            return false;
                        }}
                    </script>
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

    let page = Page(stacktraces).to_html()?;

    println!("{}", page);

    Ok(())
}
