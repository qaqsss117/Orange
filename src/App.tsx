export default function App() {
  return (
    <main className="startup-shell">
      <section
        className="startup-status"
        aria-labelledby="product-name"
        aria-live="polite"
      >
        <div className="brand-mark" aria-hidden="true">
          O
        </div>
        <h1 id="product-name">Orange</h1>
        <p>
          <span className="status-indicator" aria-hidden="true" />
          正在初始化安全连接
        </p>
      </section>
    </main>
  );
}
