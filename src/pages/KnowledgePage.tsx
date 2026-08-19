import DOMPurify from "dompurify";
import {
  AlertCircle,
  ArrowLeft,
  BookOpen,
  FileText,
  RefreshCw,
  Search,
} from "lucide-react";
import { useEffect, useState } from "react";
import type {
  KnowledgeDetailResponse,
  KnowledgeListResponse,
} from "../businessApi";
import { toPublicUiError, type ShellServices } from "../shellServices";

function formatUpdatedAt(value: number | null): string {
  if (value === null) return "";
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(new Date(value));
}

export function KnowledgePage({ services }: { services: ShellServices }) {
  const [list, setList] = useState<KnowledgeListResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [keyword, setKeyword] = useState("");
  const [searching, setSearching] = useState(false);
  const [activeCategory, setActiveCategory] = useState<string | null>(null);
  const [detail, setDetail] = useState<KnowledgeDetailResponse | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);

  useEffect(() => {
    let active = true;
    void services.fetchKnowledgeList().then(
      (response) => {
        if (active) {
          setList(response);
          setLoading(false);
        }
      },
      (reason) => {
        if (active) {
          setError(toPublicUiError(reason).message);
          setLoading(false);
        }
      },
    );
    return () => {
      active = false;
    };
  }, [services]);

  const search = async () => {
    if (searching) return;
    setSearching(true);
    setError(null);
    try {
      const response = await services.fetchKnowledgeList(
        keyword.trim() === "" ? undefined : keyword.trim(),
      );
      setList(response);
      setActiveCategory(null);
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setSearching(false);
    }
  };

  const openArticle = async (articleId: string) => {
    if (detailLoading) return;
    setDetailLoading(true);
    setError(null);
    try {
      setDetail(await services.fetchKnowledgeDetail(articleId));
    } catch (reason) {
      setError(toPublicUiError(reason).message);
    } finally {
      setDetailLoading(false);
    }
  };

  if (detail !== null) {
    return (
      <main className="management-page knowledge-page">
        <header className="management-heading">
          <div>
            <span>{detail.category ?? "文档中心"}</span>
            <h2>{detail.title}</h2>
            <p>{formatUpdatedAt(detail.updatedAtUnixMs)}</p>
          </div>
          <button
            type="button"
            className="secondary-action"
            onClick={() => setDetail(null)}
          >
            <ArrowLeft aria-hidden="true" />
            返回列表
          </button>
        </header>
        <article
          className="knowledge-body"
          // 服务端返回的文档 HTML，经 DOMPurify 消毒后渲染
          dangerouslySetInnerHTML={{
            __html: DOMPurify.sanitize(detail.bodyHtml),
          }}
        />
      </main>
    );
  }

  const groups = list?.groups ?? [];
  const categories = groups.map((group) => group.category);
  const visibleGroups =
    activeCategory === null
      ? groups
      : groups.filter((group) => group.category === activeCategory);

  return (
    <main className="management-page knowledge-page">
      <header className="management-heading">
        <div>
          <span>帮助中心</span>
          <h2>文档中心</h2>
          <p>使用教程与常见问题文档。</p>
        </div>
      </header>

      <form
        className="knowledge-search"
        onSubmit={(event) => {
          event.preventDefault();
          void search();
        }}
      >
        <div className="input-shell">
          <Search aria-hidden="true" />
          <input
            type="text"
            value={keyword}
            maxLength={128}
            placeholder="搜索文档标题与内容"
            disabled={searching}
            onChange={(event) => setKeyword(event.target.value)}
          />
        </div>
        <button type="submit" className="secondary-action" disabled={searching}>
          {searching ? "正在搜索" : "搜索"}
        </button>
      </form>

      {categories.length > 1 && (
        <div className="knowledge-categories" role="tablist">
          <button
            type="button"
            role="tab"
            aria-selected={activeCategory === null}
            className={
              activeCategory === null
                ? "knowledge-category knowledge-category-active"
                : "knowledge-category"
            }
            onClick={() => setActiveCategory(null)}
          >
            全部
          </button>
          {categories.map((category) => (
            <button
              type="button"
              role="tab"
              key={category}
              aria-selected={activeCategory === category}
              className={
                activeCategory === category
                  ? "knowledge-category knowledge-category-active"
                  : "knowledge-category"
              }
              onClick={() => setActiveCategory(category)}
            >
              {category}
            </button>
          ))}
        </div>
      )}

      {loading ? (
        <div className="page-state" role="status">
          <RefreshCw className="spinning" aria-hidden="true" />
          <span>正在读取文档</span>
        </div>
      ) : error !== null && list === null ? (
        <div className="page-state page-state-error" role="alert">
          <AlertCircle aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : visibleGroups.length === 0 ? (
        <div className="page-state" role="status">
          <BookOpen aria-hidden="true" />
          <strong>暂无文档</strong>
          <span>没有找到匹配的文档内容。</span>
        </div>
      ) : (
        <>
          {error !== null && (
            <div className="inline-notice inline-notice-error" role="alert">
              <AlertCircle aria-hidden="true" />
              <span>{error}</span>
            </div>
          )}
          {visibleGroups.map((group) => (
            <section
              className="knowledge-group"
              key={group.category}
              aria-label={group.category}
            >
              <h3>{group.category}</h3>
              <ul>
                {group.articles.map((article) => (
                  <li key={article.articleId}>
                    <button
                      type="button"
                      disabled={detailLoading}
                      onClick={() => void openArticle(article.articleId)}
                    >
                      <FileText aria-hidden="true" />
                      <span>
                        <strong>{article.title}</strong>
                        <small>
                          {formatUpdatedAt(article.updatedAtUnixMs)}
                        </small>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          ))}
        </>
      )}
    </main>
  );
}
