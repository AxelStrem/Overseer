div SimpleTest {
    text PlainText = "This is plain text without any formatting."
    text MarkdownText (markdown=true) = "This is **bold** and *italic* text with `code`."
    text MoreMarkdown (markdown=true, font-size=20px) = "# A heading with *emphasis*"
}
