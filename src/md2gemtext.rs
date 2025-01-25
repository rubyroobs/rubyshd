// Forking from https://git.sr.ht/~boringcactus/md2gemtext which hasn't been updated for 4 years...
// changes probably aren't worth republishing? idk...
//
// changelog:
// - updated to use newer version of pulldown-cmark
// - (updated tests to account for html block, reworked link/image handling, gemtext qualified as gmi)
//
use gemtext as gmi;
use pulldown_cmark as md;

/// Converts a given string of Markdown to semi-equivalent gemtext.
///
/// # Panics
///
/// Will panic if gemtext::render somehow produces invalid UTF-8.
/// Since gemtext::render only produces valid UTF-8, this should never happen.
pub fn convert(markdown_text: &str) -> String {
    let parser = md::Parser::new_ext(markdown_text, md::Options::empty());
    let mut state = State::new();

    for event in parser {
        match event {
            md::Event::Start(tag) => match tag {
                md::Tag::Paragraph => (),
                md::Tag::Heading { level, .. } => state.start_heading(level),
                md::Tag::BlockQuote(_) => state.start_block_quote(),
                md::Tag::CodeBlock(_) => state.start_code_block(),
                md::Tag::List(_) => (),
                md::Tag::Item => state.start_list_item(),
                md::Tag::FootnoteDefinition(_) => {
                    unimplemented!("footnotes disabled")
                }
                md::Tag::Table(_) => unimplemented!("tables disabled"),
                md::Tag::TableHead => unimplemented!("tables disabled"),
                md::Tag::TableRow => unimplemented!("tables disabled"),
                md::Tag::TableCell => unimplemented!("tables disabled"),
                md::Tag::Emphasis => state.toggle_emphasis(),
                md::Tag::Strong => state.toggle_strong(),
                md::Tag::Strikethrough => unimplemented!("strikethrough disabled"),
                md::Tag::Link { dest_url, .. } => state.start_link(&dest_url),
                md::Tag::Image { dest_url, .. } => state.start_image(&dest_url),
                md::Tag::HtmlBlock => (),
                md::Tag::DefinitionList => unimplemented!("definition list disabled"),
                md::Tag::DefinitionListTitle => unimplemented!("definition list disabled"),
                md::Tag::DefinitionListDefinition => unimplemented!("definition list disabled"),
                md::Tag::MetadataBlock(_) => unimplemented!("metadata block disabled"),
            },
            md::Event::End(tag) => match tag {
                md::TagEnd::Paragraph => state.finish_node(),
                md::TagEnd::Heading(_) => state.finish_node(),
                md::TagEnd::BlockQuote(_) => (),
                md::TagEnd::CodeBlock => state.finish_node(),
                md::TagEnd::List(_) => state.finish_list(),
                md::TagEnd::Item => state.finish_node(),
                md::TagEnd::FootnoteDefinition => {
                    unimplemented!("footnotes disabled")
                }
                md::TagEnd::Table => unimplemented!("tables disabled"),
                md::TagEnd::TableHead => unimplemented!("tables disabled"),
                md::TagEnd::TableRow => unimplemented!("tables disabled"),
                md::TagEnd::TableCell => unimplemented!("tables disabled"),
                md::TagEnd::Emphasis => state.toggle_emphasis(),
                md::TagEnd::Strong => state.toggle_strong(),
                md::TagEnd::Strikethrough => unimplemented!("strikethrough disabled"),
                md::TagEnd::Link => state.finish_link(),
                md::TagEnd::Image => state.finish_image(),
                md::TagEnd::HtmlBlock => state.finish_node(),
                md::TagEnd::DefinitionList => unimplemented!("definition list disabled"),
                md::TagEnd::DefinitionListTitle => unimplemented!("definition list disabled"),
                md::TagEnd::DefinitionListDefinition => unimplemented!("definition list disabled"),
                md::TagEnd::MetadataBlock(_) => unimplemented!("metadata block disabled"),
            },
            md::Event::Text(text) => state.add_text(&text),
            md::Event::Code(code) => state.add_inline_code(&code),
            md::Event::Html(html) => state.add_text(&html),
            md::Event::FootnoteReference(_) => unimplemented!("footnotes disabled"),
            md::Event::SoftBreak => state.add_text(" "),
            md::Event::HardBreak => state.finish_node(),
            md::Event::Rule => state.add_rule(),
            md::Event::TaskListMarker(_) => unimplemented!("task lists disabled"),
            md::Event::InlineMath(_) => unimplemented!("inline math disabled"),
            md::Event::DisplayMath(_) => unimplemented!("display math disabled"),
            md::Event::InlineHtml(_) => unimplemented!("inline html disabled"),
        }
    }

    let nodes = state
        .nodes
        .into_iter()
        .filter(|cluster| !cluster.is_empty())
        .map(condense)
        .collect::<Vec<_>>()
        .join(&gmi::Node::blank());
    let mut result: Vec<u8> = vec![];
    gmi::render(nodes, &mut result).expect("gemtext::render somehow failed");
    String::from_utf8(result).expect("gemtext::render somehow produced invalid UTF-8")
}

type NodeCluster = Vec<gmi::Node>;

fn condense(original: NodeCluster) -> NodeCluster {
    match original.as_slice() {
        [gmi::Node::Text(text), gmi::Node::Link {
            name: Some(name), ..
        }] if text == name => vec![original[1].clone()],
        _ => original,
    }
}

enum NodeType {
    Text,
    Preformatted,
    Heading { level: u8 },
    ListItem,
    Quote,
}

impl NodeType {
    fn take(&mut self) -> Self {
        std::mem::replace(self, NodeType::Text)
    }

    fn construct(self, body: String) -> gmi::Node {
        use NodeType::*;
        match self {
            Text => gmi::Node::Text(body),
            Preformatted => gmi::Node::Preformatted(body),
            Heading { level } => gmi::Node::Heading { level, body },
            ListItem => gmi::Node::ListItem(body),
            Quote => gmi::Node::Quote(body),
        }
    }
}

struct State {
    nodes: Vec<NodeCluster>,
    pending_node_content: String,
    pending_node_type: NodeType,
    pending_links: Vec<gmi::Node>,
    link_text_stack: Vec<String>,
}

impl State {
    fn new() -> Self {
        State {
            nodes: vec![],
            pending_node_content: String::new(),
            pending_node_type: NodeType::Text,
            pending_links: vec![],
            link_text_stack: vec![],
        }
    }

    fn start_heading(&mut self, level: md::HeadingLevel) {
        let level = match level {
            md::HeadingLevel::H1 => 1,
            md::HeadingLevel::H2 => 2,
            _ => 3,
        };
        self.pending_node_type = NodeType::Heading { level };
    }

    fn start_block_quote(&mut self) {
        self.pending_node_type = NodeType::Quote;
    }

    fn start_code_block(&mut self) {
        self.pending_node_type = NodeType::Preformatted;
    }

    fn start_list_item(&mut self) {
        self.pending_node_type = NodeType::ListItem;
    }

    fn toggle_emphasis(&mut self) {
        self.add_text("_");
    }

    fn toggle_strong(&mut self) {
        self.add_text("**");
    }

    fn start_link(&mut self, dest_url: &str) {
        self.link_text_stack.push(String::new());
        self.pending_links.push(gmi::Node::Link {
            to: dest_url.to_string(),
            name: Some(dest_url.to_string()),
        });
    }

    fn start_image(&mut self, dest_url: &str) {
        self.link_text_stack.push(String::new());
        self.pending_links.push(gmi::Node::Link {
            to: dest_url.to_string(),
            name: Some(dest_url.to_string()),
        });
        self.pending_node_content += "[image: ";
    }

    fn finish_link(&mut self) {
        if let Some(text) = self.link_text_stack.pop() {
            if let Some(pending_link) = self.pending_links.pop() {
                if let gmi::Node::Link { to, .. } = pending_link {
                    self.pending_links.push(gmi::Node::Link {
                        to: to,
                        name: Some(text),
                    })
                }
            }
        }
    }

    fn finish_image(&mut self) {
        if let Some(text) = self.link_text_stack.pop() {
            if let Some(pending_link) = self.pending_links.pop() {
                if let gmi::Node::Link { to, .. } = pending_link {
                    self.pending_links.push(gmi::Node::Link {
                        to: to,
                        name: Some(format!("[image: {}]", text)),
                    })
                }
            }
        }
        self.pending_node_content += "]";
    }

    fn finish_list(&mut self) {
        self.nodes.push(vec![]);
    }

    // will create an empty paragraph if pending_text is empty
    fn finish_node(&mut self) {
        match (
            &self.pending_node_type,
            self.nodes.last().and_then(|cluster| cluster.last()),
        ) {
            (NodeType::ListItem, Some(gmi::Node::ListItem(_))) => (),
            _ => self.nodes.push(vec![]),
        }
        let node_text = self.pending_node_content.trim().to_string();
        let new_node = self.pending_node_type.take().construct(node_text);
        let last_cluster = self.nodes.last_mut().expect("empty cluster list??");
        last_cluster.push(new_node);
        last_cluster.extend(self.pending_links.drain(..));

        self.pending_node_content = String::new();
    }

    fn add_text(&mut self, text: &str) {
        for link_text in &mut self.link_text_stack {
            *link_text += text;
        }
        self.pending_node_content += text;
    }

    fn add_inline_code(&mut self, code: &str) {
        self.pending_node_content += "`";
        self.pending_node_content += code;
        self.pending_node_content += "`";
    }

    fn add_rule(&mut self) {
        self.add_text("-----");
        self.finish_node();
    }
}

#[cfg(test)]
#[test]
fn test_kitchen_sink() {
    let markdown_demo = r#"
# h1
## h2
### h3
<p>looks like html</p>

---

```
sample
  text
```

> implying

1. don't pick up the phone
2. don't let him in
3. don't be his friend

some `code` and some `` fancy`code `` and *italics*
and __bold__ and ***semi-overlapping* bold *and* italics**

this paragraph has [one link](http://example.net)

this [paragraph](http://example.com) has [several links](http://example.org)
and an ![inline image](a://url) in it

![this one's just an image](https://placekitten.com/200/300)
"#;
    let gemtext_demo = r#"# h1

## h2

### h3

<p>looks like html</p>

-----

```
sample
  text
```

> implying

* don't pick up the phone
* don't let him in
* don't be his friend

some `code` and some `fancy`code` and _italics_ and **bold** and **_semi-overlapping_ bold _and_ italics**

this paragraph has one link
=> http://example.net one link

this paragraph has several links and an [image: inline image] in it
=> http://example.com paragraph
=> http://example.org several links
=> a://url [image: inline image]

=> https://placekitten.com/200/300 [image: this one's just an image]
"#;
    assert_eq!(convert(markdown_demo), gemtext_demo);
}

#[cfg(test)]
#[test]
fn test_list_start() {
    let markdown = "> hi\n\n1. uh\n2. ah\n";
    let gemtext = "> hi\n\n* uh\n* ah\n";
    assert_eq!(convert(markdown), gemtext);
}
