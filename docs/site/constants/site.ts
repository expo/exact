// The site's public origin, such as "https://docs.example.com": the sitemap, canonical links and JSON-LD URLs
// are absolute, so they are written only when it is set. SITE_ORIGIN overrides it for one build.
const ORIGIN = 'https://exact.expo.dev';

export const site = {
  origin: (process.env.SITE_ORIGIN ?? ORIGIN).replace(/\/+$/, ''),
  name: 'Exact Docs',
  locale: 'en_US',
  shareImage: '/assets/og-image.png',
  description:
    'Exact is an all-platform framework for real apps on the web, iOS, macOS, Android, Linux, and more.',
};
