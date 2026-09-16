//! 投稿版 docx 的 XML 片段与样式。
//!
//! # 样式层统一下发，正文里不塞空格
//!
//! 「首行缩进两字符」写在 `styles.xml` 的 `Normal` 里，正文段落里**一个缩进空格都不写**。
//! 属性**两个都给**：
//! - `w:firstLineChars=\"200\"`（一字符的百分之一为单位，两百＝两字符）是主：按 ECMA-376，
//!   两者同时出现时 **`firstLineChars` 优先**，所以字号一改，缩进跟着变两格，不会跑版；
//! - `w:firstLine=\"420\"`（五号 10.5pt 下两字符＝420 twips）是兜底：有些阅读器
//!   **不解析 `*Chars` 变体**（LibreOffice 一直如此，WPS 同类），只给字符单位它们干脆不缩进，
//!   投稿稿到了编辑手里就是一篇没有段首缩进的稿子。
//!
//! 包里另外给齐 `settings.xml`（写死 `compatibilityMode=15`）与 `docProps`：缺了这几件，
//! Word / WPS 会把这稿子当**兼容模式**打开（版式默认值不一样）。
//! ⚠️ `docProps/core.xml` **不写时间戳**——那会让「编译两遍逐字节一样」失效（导出幂等铁律）。
//!
//! 字体只写**字体名**（宋体 / Times New Roman），不内置字体文件：字由作者的 Word 提供，
//! 这样既不违反字体纪律（不内置商业字体），也不用把几 MB 字体塞进安装包。
//!
//! 中文字号对照：五号 = 10.5pt = `w:sz` 21（半磅），四号 = 14pt = 28，小二号 = 18pt = 36。
// i18n-allow-file: 本文件里的中文是**写进投稿文件的内容**（书名下的"简介""大纲"小标题与字体名），
// 不是界面文案——它要跟着稿子走，不随界面语言变；界面文案在 locales/zh-Hans.json。

/// 投稿包里大纲那一节的标题（**写进文件的内容**，跟稿子走，不随界面语言变）。
pub(crate) const OUTLINE_HEADING: &str = "大纲";

/// 投稿包里故事总纲那一节的标题（同上：写进文件的内容）。
///
/// 与「大纲」分开两节：大纲是**逐章清单**（一章一行），总纲是**整本书那几段**——
/// 编辑读稿时先看总纲知道这本书要干什么，再顺着大纲看每章怎么落。
pub(crate) const STORYLINE_HEADING: &str = "故事总纲";

/// 一个段落：可选样式 + 转义后的文字。
pub(crate) fn paragraph(style: Option<&str>, text: &str) -> String {
    let props = match style {
        Some(name) => format!("<w:pPr><w:pStyle w:val=\"{name}\"/></w:pPr>"),
        None => String::new(),
    };
    format!("<w:p>{props}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>", escape(text))
}

/// XML 文本转义；顺手把 XML 1.0 不接受的控制字符剔掉（正文里混进一个就会让 Word 打不开）。
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push('\t'),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
    out
}

/// 正文（document.xml）：书名 → 简介 → 正文 →（故事总纲）→ 大纲。
pub(crate) fn document(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
         <w:body>{body}<w:sectPr>\
         <w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" \
         w:header=\"851\" w:footer=\"992\" w:gutter=\"0\"/>\
         </w:sectPr></w:body></w:document>\n"
    )
}

/// 包的内容类型表。
pub(crate) const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>\
<Override PartName=\"/word/settings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml\"/>\
<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>\
<Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>\
</Types>\n";

/// 包的根关系：谁是主文档。
pub(crate) const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" \
Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" \
Target=\"word/document.xml\"/>\
<Relationship Id=\"rId2\" \
Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" \
Target=\"docProps/core.xml\"/>\
<Relationship Id=\"rId3\" \
Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" \
Target=\"docProps/app.xml\"/>\
</Relationships>\n";

/// 主文档的关系：样式表在哪。
pub(crate) const DOC_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" \
Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" \
Target=\"styles.xml\"/>\
<Relationship Id=\"rId2\" \
Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" \
Target=\"settings.xml\"/>\
</Relationships>\n";

/// 文档设置：写死 `compatibilityMode=15`（Word 2013+ 的版式默认值）。
///
/// 缺这一件，Word / WPS 会把这稿子按**兼容模式**打开——版式默认值与编辑那边不一样，
/// 投稿稿最怕的就是「我这边看着好好的」。
pub(crate) const SETTINGS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<w:settings xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:zoom w:percent=\"100\"/>\
<w:defaultTabStop w:val=\"420\"/>\
<w:characterSpacingControl w:val=\"compressPunctuation\"/>\
<w:compat>\
<w:compatSetting w:name=\"compatibilityMode\" \
w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"15\"/>\
</w:compat>\
</w:settings>\n";

/// 核心属性：**只写书名，不写时间戳**（时间戳会让「编译两遍字节一样」失效）。
pub(crate) fn core_properties(title: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
         <cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" \
         xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:title>{}</dc:title>\
         </cp:coreProperties>\n",
        escape(title)
    )
}

/// 扩展属性：谁生成的（除程序名外不写任何机器信息）。
pub(crate) const APP_PROPERTIES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\">\
<Application>Yanmo</Application>\
</Properties>\n";

/// 样式表：`Normal`（正文，首行缩进两字符、五号、1.5 倍行距）与几个标题样式。
pub(crate) const STYLES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n\
<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\">\
<w:name w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:ind w:firstLine=\"420\" w:firstLineChars=\"200\"/><w:spacing w:line=\"360\" w:lineRule=\"auto\"/>\
<w:jc w:val=\"both\"/></w:pPr>\
<w:rPr><w:rFonts w:ascii=\"Times New Roman\" w:hAnsi=\"Times New Roman\" w:eastAsia=\"宋体\"/>\
<w:sz w:val=\"21\"/><w:szCs w:val=\"21\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"YanmoTitle\"><w:name w:val=\"Yanmo Title\"/>\
<w:basedOn w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:ind w:firstLineChars=\"0\" w:firstLine=\"0\"/><w:jc w:val=\"center\"/>\
<w:spacing w:before=\"240\" w:after=\"240\"/></w:pPr>\
<w:rPr><w:b/><w:sz w:val=\"36\"/><w:szCs w:val=\"36\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"YanmoVolume\"><w:name w:val=\"Yanmo Volume\"/>\
<w:basedOn w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:ind w:firstLineChars=\"0\" w:firstLine=\"0\"/>\
<w:spacing w:before=\"240\" w:after=\"120\"/></w:pPr>\
<w:rPr><w:b/><w:sz w:val=\"28\"/><w:szCs w:val=\"28\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"YanmoChapter\"><w:name w:val=\"Yanmo Chapter\"/>\
<w:basedOn w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:ind w:firstLineChars=\"0\" w:firstLine=\"0\"/>\
<w:spacing w:before=\"180\" w:after=\"60\"/></w:pPr>\
<w:rPr><w:b/><w:sz w:val=\"21\"/><w:szCs w:val=\"21\"/></w:rPr></w:style>\
<w:style w:type=\"paragraph\" w:styleId=\"YanmoPlain\"><w:name w:val=\"Yanmo Plain\"/>\
<w:basedOn w:val=\"Normal\"/><w:qFormat/>\
<w:pPr><w:ind w:firstLineChars=\"0\" w:firstLine=\"0\"/><w:jc w:val=\"left\"/>\
<w:spacing w:line=\"300\" w:lineRule=\"auto\"/></w:pPr></w:style>\
</w:styles>\n";
