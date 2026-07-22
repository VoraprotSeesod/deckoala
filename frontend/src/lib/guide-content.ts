/** Slide-guide / usage-manual content (BRIEF-0009d).
 *
 * Shared by the standalone `/app/guide` manual and the editor's SlideGuide
 * modal. Prose/labels are message KEYS (t()); the Marp snippets are LITERAL
 * code — identical in both languages, so they can never render as a raw key.
 */

export type GuideSnippet = {
	/** Message key for the snippet's caption. */
	labelKey: string;
	/** Literal Marp Markdown. Never translated. */
	code: string;
};

export type GuideSection = {
	/** Message key for the section heading. */
	titleKey: string;
	/** Message key for a short intro paragraph. */
	introKey: string;
	snippets: GuideSnippet[];
};

export const GUIDE: GuideSection[] = [
	{
		titleKey: 'guide.structure.title',
		introKey: 'guide.structure.intro',
		snippets: [
			{
				labelKey: 'guide.structure.slides',
				code: '# Slide one\n\n---\n\n# Slide two'
			},
			{
				labelKey: 'guide.structure.text',
				code: '## Heading\n\n- bullet\n- **bold** and *italic*\n- [a link](https://example.com)\n\n> A quote\n\n`inline code`'
			}
		]
	},
	{
		titleKey: 'guide.center.title',
		introKey: 'guide.center.intro',
		snippets: [
			{
				labelKey: 'guide.center.example',
				code: '<!-- _class: center -->\n\n# Centered title\n\nThe text on this slide is centered.'
			}
		]
	},
	{
		titleKey: 'guide.layout.title',
		introKey: 'guide.layout.intro',
		snippets: [
			{
				labelKey: 'guide.layout.columns',
				code: '<!-- _class: columns -->\n\n## Left and right\n\nText flows into two columns on this slide.'
			},
			{
				labelKey: 'guide.layout.bg',
				code: '![bg](/assets/your-deck/photo.jpg)\n\n# Title over a full background'
			},
			{
				labelKey: 'guide.layout.bgSplit',
				code: '![bg left](/assets/your-deck/photo.jpg)\n\n# Image on the left, text on the right'
			}
		]
	},
	{
		titleKey: 'guide.images.title',
		introKey: 'guide.images.intro',
		snippets: [
			{
				labelKey: 'guide.images.basic',
				code: '![My photo](/assets/your-deck/photo.jpg)'
			},
			{
				labelKey: 'guide.images.sized',
				code: '![My photo w:320](/assets/your-deck/photo.jpg)\n\n![Wide banner w:640 h:200](/assets/your-deck/banner.jpg)'
			}
		]
	},
	{
		titleKey: 'guide.fonts.title',
		introKey: 'guide.fonts.intro',
		snippets: [
			{
				labelKey: 'guide.fonts.customCss',
				code: "section.thai { font-family: 'Sarabun'; }\nsection.mono { font-family: 'JetBrains Mono'; }"
			},
			{
				labelKey: 'guide.fonts.perSlide',
				code: '<!-- _class: thai -->\n\n# หน้านี้ใช้ฟอนต์ Sarabun\n\n---\n\n<!-- _class: mono -->\n\n# This slide uses a mono font'
			}
		]
	},
	{
		titleKey: 'guide.math.title',
		introKey: 'guide.math.intro',
		snippets: [
			{
				labelKey: 'guide.math.example',
				code: 'Inline math $E = mc^2$ in a sentence.\n\n$$\n\\int_0^1 x^2 \\, dx = \\frac{1}{3}\n$$'
			}
		]
	},
	{
		titleKey: 'guide.notes.title',
		introKey: 'guide.notes.intro',
		snippets: [
			{
				labelKey: 'guide.notes.example',
				code: '# Slide title\n\nWhat the audience sees.\n\n<!-- Speaker notes: only you see these in presenter view. -->'
			}
		]
	}
];
