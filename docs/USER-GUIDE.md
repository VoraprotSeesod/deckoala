# Deckoala — Slide authoring guide

The same guide is available **inside the app** at **Guide** in the top nav (or the
command palette → "Open the usage manual"), and from the **Slide guide** button in
the editor where every example has an **Insert** button.

Decks are standard [Marp](https://marp.app) Markdown, so nothing here locks you in.

## Slides & text

A line containing only `---` starts a new slide.

```markdown
# Slide one

---

# Slide two
```

Everything else is ordinary Markdown — headings, `- bullets`, `**bold**`,
`*italic*`, `[links](https://example.com)`, `> quotes`, `` `inline code` `` and
fenced code blocks.

## Center a slide

Put a class comment at the top of a slide. The leading `_` scopes it to **that
slide only**.

```markdown
<!-- _class: center -->

# Centered title

This whole slide is centered.
```

## Layout

Two columns and background images use per-slide classes — no HTML required.

```markdown
<!-- _class: columns -->

## Left and right

Text flows into two columns on this slide.
```

```markdown
![bg](/assets/your-deck/photo.jpg)

# Title over a full background
```

```markdown
![bg left](/assets/your-deck/photo.jpg)

# Image on the left, text on the right
```

## Images

Upload with the **Image** button (it can also reuse an image already in the deck),
then size images with `w:` and `h:`.

```markdown
![My photo](/assets/your-deck/photo.jpg)

![My photo w:320](/assets/your-deck/photo.jpg)

![Wide banner w:640 h:200](/assets/your-deck/banner.jpg)
```

### Reusing a figure from a paper

If you uploaded a PDF on the **Research** page, its embedded figures are pulled out
for you. In the **Image** dialog open the **From research** tab and pick one — it is
copied into the deck and the Markdown is inserted, so the chart in your slide is the
paper's own. Give it a real description in the alt text; the copy arrives with a
generated filename.

## Writing slides from your research

Upload the papers a deck draws on under **Research** (PDF, or `.txt`/`.md`). Deckoala
reads the text on the server — the file never leaves your instance except as the AI
prompt you explicitly send. When you press **AI**, tick the papers to source from and
the generated slides are built from that material instead of invented.

Because the model sees the paper's text, keep the prompt about *shape* ("a 10-slide
summary, one finding per slide") and let the research supply the facts. Always read
the result: an AI can still misattribute a number.

## A different font per slide

1. Install the font on the **Fonts** page.
2. Name a class in **Custom CSS** (the *Custom CSS* button in the editor):

   ```css
   section.thai { font-family: 'Sarabun'; }
   section.mono { font-family: 'JetBrains Mono'; }
   ```

3. Apply it to a slide with a class comment:

   ```markdown
   <!-- _class: thai -->

   # หน้านี้ใช้ฟอนต์ Sarabun

   ---

   <!-- _class: mono -->

   # This slide uses a mono font
   ```

Each slide can use a different font this way.

## Math

Write LaTeX between `$` (inline) or `$$` (block).

```markdown
Inline math $E = mc^2$ in a sentence.

$$
\int_0^1 x^2 \, dx = \frac{1}{3}
$$
```

## Speaker notes

An HTML comment becomes a speaker note — shown in **presenter view**, never on the
slide.

```markdown
# Slide title

What the audience sees.

<!-- Speaker notes: only you see these in presenter view. -->
```

## Themes & custom CSS

Pick a look with the **Theme** button (Deckoala light / dark / bold), and add
per-deck CSS with **Custom CSS**. External `url()` and `@import` are stripped from
custom CSS so shared and exported slides never make outside requests.
