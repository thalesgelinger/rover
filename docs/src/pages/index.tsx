import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import CodeBlock from '@theme/CodeBlock';

import styles from './index.module.css';
import heroSnippet from '@site/src/data/hero-snippet';

function CodeSnippet() {
  return (
    <div className={styles.heroCode}>
      <div className={styles.codeWindow}>
        <div className={styles.codeWindowHeader}>
          <span className={styles.codeWindowDot}></span>
          <span className={styles.codeWindowDot}></span>
          <span className={styles.codeWindowDot}></span>
          <span className={styles.codeWindowTitle}>main.lua</span>
        </div>
        <CodeBlock language="lua" className={styles.codeBlock}>
          {heroSnippet}
        </CodeBlock>
      </div>
    </div>
  );
}

function HomepageHeader() {
  return (
    <header className={styles.heroBanner}>
      <div className={styles.heroContainer}>
        <div className={styles.heroContent}>
          <div className={styles.logoContainer}>
            <img src="/rover/img/rover-logo.svg" alt="Rover Logo" className={styles.heroLogo} />
          </div>
          <Heading as="h1" className={styles.heroTitle}>
            Rover
          </Heading>
          <p className={styles.heroTagline}>
            Lua runtime for building <span className={styles.highlight}>REAL</span> full-stack applications
          </p>
          <p className={styles.heroDescription}>
            Build web servers, frontends, mobile apps, and desktop applications - all with Lua
          </p>
          <div className={styles.buttons}>
            <Link
              className="button button--primary button--lg"
              to="/docs/intro">
              Get Started →
            </Link>
            <Link
              className="button button--secondary button--lg"
              to="https://github.com/thalesgelinger/rover">
              GitHub
            </Link>
          </div>
          <div className={styles.platforms}>
            <span className={styles.platformBadge}>🌐 Web</span>
            <span className={styles.platformBadge}>📱 Mobile</span>
            <span className={styles.platformBadge}>🖥️ Desktop</span>
            <span className={styles.platformBadge}>⚡ Backend</span>
          </div>
        </div>
        
        <CodeSnippet />
      </div>
    </header>
  );
}

function FeaturesSection() {
  return (
    <section className={styles.features}>
      <div className={styles.container}>
        <div className={styles.featureGrid}>
          <div className={styles.feature}>
            <h3>🚀 Now: Backend Server</h3>
            <p>High-performance HTTP server with routing, middleware, and JSON support. Built for speed with zero-copy response handling.</p>
          </div>
          <div className={styles.feature}>
            <h3>🎯 Coming: Frontend</h3>
            <p>Build reactive UIs with Lua. Component-based architecture with hot reload and modern tooling.</p>
          </div>
          <div className={styles.feature}>
            <h3>📱 Coming: Mobile</h3>
            <p>Native mobile apps for iOS and Android. Share code between platforms while accessing native APIs.</p>
          </div>
          <div className={styles.feature}>
            <h3>🖥️ Coming: Desktop</h3>
            <p>Cross-platform desktop applications for macOS, Windows, and Linux with native performance.</p>
          </div>
        </div>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title="Lua runtime for full-stack applications"
      description="Build web servers, frontends, mobile, and desktop apps with Lua">
      <HomepageHeader />
      <main>
        <FeaturesSection />
      </main>
    </Layout>
  );
}
