div MarkdownDemo (font-size=16px, background-color=#F8F9FA) {
    
    text IntroText (markdown=true) = "# Markdown Demo

This is a **bold** statement and this is *italic* text.

## Features Supported

- **Headings** (H1-H6)
- **Bold** and *italic* text
- Lists (bullet and numbered)
- `inline code`
- [Links](https://example.com)
- > Blockquotes

### Code Example

```javascript
console.log('Hello, Markdown!');
```

Double-click this text to edit in markdown mode!"
    
    text PlainText = "This is plain text without markdown formatting.
        
**This won't be bold** and *this won't be italic*.

Double-click to edit as plain text."
    
    text ShortMarkdown (markdown=true, font-size=18px, font-color=blue) = "**Quick markdown note:** This supports *formatting* and `code`!"
}
