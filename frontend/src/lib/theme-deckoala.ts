// The instance's default Marp theme — brand colors + Thai-capable fonts.
// Registered into marp-core's theme set in $lib/marp.ts (BRIEF-0003).
export const themeDeckoala = `/* @theme deckoala */
@import 'default';

section {
	background: #f8f8ff;
	color: #0b1215;
	font-family: 'Inter', 'Noto Sans Thai', system-ui, sans-serif;
	font-size: 28px;
	line-height: 1.55;
	padding: 64px;
}

h1,
h2,
h3,
h4,
h5,
h6 {
	color: #0b1215;
	line-height: 1.25;
}

h1 {
	font-size: 1.9em;
}

a {
	color: #0b1215;
	text-decoration: underline;
}

code {
	background: rgba(11, 18, 21, 0.08);
	color: #0b1215;
	padding: 0.1em 0.35em;
	border-radius: 4px;
}

pre {
	background: rgba(11, 18, 21, 0.06);
	padding: 0.7em 1em;
	border-radius: 8px;
}

pre code {
	background: transparent;
	padding: 0;
}

blockquote {
	border-left: 4px solid rgba(11, 18, 21, 0.25);
	padding-left: 0.8em;
	color: rgba(11, 18, 21, 0.75);
}

table th {
	background: rgba(11, 18, 21, 0.08);
}

section::after {
	color: rgba(11, 18, 21, 0.45);
}
`;
