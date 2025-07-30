import '../styles/main.css';

class AboutPage {
  private contentContainer: HTMLElement;

  constructor() {
    this.contentContainer = document.getElementById('content')!;
    this.loadContent();
  }

  private loadContent() {
    this.contentContainer.innerHTML = `
      <p>Axismundi is a modern web application built with:</p>
      <ul>
        <li><strong>Backend:</strong> Rust with Axum framework</li>
        <li><strong>Database:</strong> PostgreSQL with SQLx</li>
        <li><strong>Frontend:</strong> TypeScript with route-based splitting</li>
        <li><strong>Build System:</strong> esbuild for fast compilation</li>
      </ul>
      
      <h2>Features</h2>
      <ul>
        <li>RESTful API with JSON responses</li>
        <li>User management system</li>
        <li>Route-based frontend architecture</li>
        <li>Hot reloading during development</li>
        <li>Static file serving</li>
      </ul>
      
      <h2>Architecture</h2>
      <p>This application demonstrates a clean separation between backend and frontend, 
      with the Rust backend serving both API endpoints and static HTML/JS files. 
      Each frontend route is built as a separate bundle, allowing for efficient loading 
      and caching strategies.</p>
    `;
  }
}

document.addEventListener('DOMContentLoaded', () => {
  new AboutPage();
});