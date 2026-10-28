// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

export default defineConfig({
	site: 'https://jonmchan.github.io',
	base: '/agent-msg-relay',
	integrations: [
		starlight({
			title: 'agent-msg-relay',
			description:
				'Any agent can pass a message to any other agent. One shared path, whichever platform each agent runs on.',
			credits: false,
			favicon: '/favicon.svg',
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/jonmchan/agent-msg-relay',
				},
			],
			sidebar: [
				{ label: 'Protocol', slug: 'protocol' },
				{ label: 'Cryptography', slug: 'crypto' },
				{ label: 'Command line', slug: 'cli' },
				{ label: 'Skill', slug: 'skill-reference' },
				{ label: 'Limits', slug: 'limits' },
				{ label: 'Security', slug: 'security' },
				{ label: 'Release', slug: 'release' },
				{
					label: 'Architecture decisions',
					items: [{ autogenerate: { directory: 'adr' } }],
				},
			],
			customCss: ['./src/styles.css'],
			components: {
				Hero: './src/components/Hero.astro',
			},
		}),
	],
});
