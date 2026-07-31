# Schl8 Sample Document

This embedded sample exercises the markdown renderer. It is compiled into
**debug builds only** and contains no secrets.

## Inline styles

Regular text with **bold emphasis**, *italics*, ~~strikethrough~~, and
`inline code`. Links render styled but are deliberately not clickable:
[example link](https://example.com).

## Lists

- First bullet
- Second bullet with `code`
  - Nested bullet
  - Another nested one

1. Ordered item
2. Second item
   1. Nested ordered

### Task list

- [x] Fix secure memory bugs
- [x] Render markdown
- [ ] Directory browsing

## Code

```rust
fn main() {
    println!("plaintext never touches disk");
}
```

## Quote

> Security is a process, not a product.
> This quote spans two source lines.

## Table

| Feature | Status |
|---------|--------|
| Markdown | done |
| Drag & drop | done |
| Auto-lock | planned |

---

That horizontal rule above ends the sample.
