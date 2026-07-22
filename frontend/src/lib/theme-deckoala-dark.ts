// Dark Deckoala theme — ink background for dark-room talks (BRIEF-0009c).
// Same Thai-capable font stack as the light theme, so it stays zero-external
// and renders Thai. The APP dark mode is separate; this is a SLIDE theme a deck
// opts into via `theme: deckoala-dark`.
export const themeDeckoalaDark = `/* @theme deckoala-dark */
@import 'default';

section {
	background: #0b1215;
	color: #f2f4f6;
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
	color: #ffffff;
	line-height: 1.25;
}

h1 {
	font-size: 1.9em;
}

a {
	color: #9db8ff;
	text-decoration: underline;
}

code {
	background: rgba(255, 255, 255, 0.12);
	color: #f2f4f6;
	padding: 0.1em 0.35em;
	border-radius: 4px;
}

pre {
	background: rgba(255, 255, 255, 0.08);
	padding: 0.7em 1em;
	border-radius: 8px;
}

pre code {
	background: transparent;
	padding: 0;
}

blockquote {
	border-left: 4px solid rgba(242, 244, 246, 0.35);
	padding-left: 0.8em;
	color: rgba(242, 244, 246, 0.78);
}

table th {
	background: rgba(255, 255, 255, 0.1);
}

section::after {
	color: rgba(242, 244, 246, 0.5);
}

section.columns {
	column-count: 2;
	column-gap: 1.5em;
}

section.center {
	text-align: center;
}

section.center img {
	margin-left: auto;
	margin-right: auto;
}
`;
