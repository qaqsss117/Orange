import { BookOpen, CircleHelp, LifeBuoy } from "lucide-react";
import { Link } from "react-router-dom";

interface HelpEntry {
  question: string;
  answer: string;
}

const HELP_ENTRIES: readonly HelpEntry[] = [
  {
    question: "软件无法正常使用？",
    answer:
      "1：确认网络没有问题，包括有线网络和 Wi-Fi 可用。\n2：确认电脑可以正常访问网页，可以尝试用浏览器打开一个常用网站，验证网络连接是否正常。",
  },
  {
    question: "初次连接提示创建 VPN 连接失败？",
    answer:
      "这种情况可能是 VPN 所需的系统权限被禁止了。可以尝试以管理员身份运行客户端后重试，若还有问题请随时联系客服。",
  },
  {
    question: "指定的国家或地区连接不上？",
    answer:
      "每个国家或地区的背后都有成百上千的网络节点在进行智能调控，若遇到指定地区连接不上的情况请随时联系客服为您定位原因，请放心一定能解决！",
  },
  {
    question: "连接成功了，但无法访问外网？",
    answer:
      "可以先在「连接设置」中切换连接模式（系统代理 / TUN）后重试。若仍然无法访问，请尽快联系客服复现问题。",
  },
  {
    question: "遇到 VPN 无法连接怎么办？",
    answer:
      "1：确定您所使用的客户端是最新版本。\n2：尝试重启客户端。\n3：联系我们的客服。",
  },
];

export function HelpPage() {
  return (
    <main className="management-page help-page">
      <header className="management-heading">
        <div>
          <span>帮助中心</span>
          <h2>问题解答</h2>
          <p>常见连接问题与排查方法。</p>
        </div>
      </header>

      <section className="help-list" aria-label="常见问题">
        {HELP_ENTRIES.map((entry) => (
          <article className="help-entry" key={entry.question}>
            <header>
              <CircleHelp aria-hidden="true" />
              <h3>{entry.question}</h3>
            </header>
            {entry.answer.split("\n").map((line) => (
              <p key={line}>{line}</p>
            ))}
          </article>
        ))}
      </section>

      <section className="help-contact" aria-labelledby="help-docs-title">
        <BookOpen aria-hidden="true" />
        <div>
          <h3 id="help-docs-title">文档中心</h3>
          <p>
            查看<Link to="/knowledge">使用教程与文档</Link>
            ，了解各平台客户端的配置方法。
          </p>
        </div>
      </section>

      <section className="help-contact" aria-labelledby="help-contact-title">
        <LifeBuoy aria-hidden="true" />
        <div>
          <h3 id="help-contact-title">仍未解决？</h3>
          <p>
            提交<Link to="/tickets">工单</Link>，我们会尽快为您处理。
          </p>
        </div>
      </section>
    </main>
  );
}
