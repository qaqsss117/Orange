import { ArrowLeft, FileText, ShieldCheck } from "lucide-react";
import { Link, useSearchParams } from "react-router-dom";

type LegalDocumentId = "terms_of_service" | "privacy_policy";

interface LegalSection {
  title: string;
  paragraphs?: readonly string[];
  bullets?: readonly string[];
}

interface LegalDocumentContent {
  eyebrow: string;
  title: string;
  summary: string;
  notice: string;
  sections: readonly LegalSection[];
}

const EFFECTIVE_DATE = "2026年8月13日";

const TERMS: LegalDocumentContent = {
  eyebrow: "服务条款",
  title: "百夫长隐私VPN用户协议",
  summary:
    "本协议约定您使用百夫长隐私VPN客户端、网站、节点网络、账户和相关支持服务时，您与本服务运营方之间的权利与义务。",
  notice:
    "请在注册、购买或使用服务前完整阅读。点击同意、创建账户、购买订阅或继续使用服务，即表示您理解并接受本协议。",
  sections: [
    {
      title: "1. 协议主体与适用范围",
      paragraphs: [
        "“百夫长隐私VPN”“本服务”或“我们”是指由客户端、官方网站及订单页面公示的实际运营主体提供的 VPN 软件和相关服务。“您”是指注册、购买或使用本服务的个人或组织。",
        "本协议同时适用于客户端、账户系统、订阅套餐、节点连接、客户支持、升级服务及我们明确纳入本协议的其他功能。第三方网站、应用和支付渠道适用其各自条款。",
      ],
    },
    {
      title: "2. 使用资格与协议接受",
      bullets: [
        "您应达到所在地法律规定的完全民事行为能力年龄；未成年人应在监护人阅读并同意后使用。",
        "代表组织使用本服务时，您确认有权使该组织受本协议约束。",
        "您应遵守所在地、节点所在地及访问目标所在地可能适用的法律法规，并自行判断 VPN 服务是否合法可用。",
      ],
    },
    {
      title: "3. 服务内容与可用性",
      paragraphs: [
        "本服务通过系统代理、TUN 或其他受支持方式，将符合路由规则的网络流量传输至所选或自动分配的节点。具体节点、协议、带宽、流量额度、设备数量和功能以客户端及购买页面展示为准。",
        "网络质量会受到用户网络、运营商、设备、节点负载、目标服务及监管环境影响。我们会合理维护服务，但不保证始终不中断，也不保证固定 IP、特定地理位置、最低速度、延迟或任何第三方内容一定可访问。",
      ],
    },
    {
      title: "4. 账户与安全",
      bullets: [
        "注册信息应真实、准确并及时更新。您应妥善保护密码、验证码、订阅链接和设备访问权限。",
        "除套餐明确允许外，不得出售、出租、转让、共享账户或绕过设备、流量及并发限制。",
        "发现账户被盗、异常登录或订阅链接泄露时，应立即修改凭据并联系客户支持。因您未合理保管凭据造成的损失，由您依法承担相应责任。",
      ],
    },
    {
      title: "5. 订阅、计费与退款",
      paragraphs: [
        "套餐价格、周期、流量、续费方式和优惠条件以结算页面为准。税费、汇率和支付渠道费用可能依地区及渠道不同而变化。除非结算页面明确说明，订阅到期后不会当然获得免费延期。",
        "退款申请按照购买页面公示规则、支付渠道规则及适用的消费者保护法律处理。已大量使用流量、违反本协议、滥用退款或使用一次性数字权益的订单，可能在法律允许范围内不予退款。",
      ],
    },
    {
      title: "6. 可接受使用规则",
      paragraphs: [
        "您不得利用本服务实施、协助或掩盖违法、有害或侵犯他人权益的活动，包括但不限于：",
      ],
      bullets: [
        "攻击、扫描、入侵或干扰网络、设备、账户和服务，传播恶意软件，实施拒绝服务攻击或规避安全控制。",
        "发送垃圾信息、钓鱼、诈骗、勒索、冒充他人，传播违法内容或侵犯知识产权、隐私权及其他合法权益。",
        "制作、获取或传播儿童性虐待材料，实施人口贩运、恐怖主义、暴力威胁或其他严重违法行为。",
        "未经授权进行高强度自动化、批量注册、爬取、转售带宽，或以持续占满资源等方式影响其他用户。",
        "规避制裁、出口管制、法院命令或服务提供方依法实施的访问限制。",
      ],
    },
    {
      title: "7. 软件、更新与系统设置",
      paragraphs: [
        "我们授予您一项有限、可撤销、非独占、不可转授权的许可，仅用于在支持的设备上安装和使用客户端。软件及品牌相关知识产权归我们或许可方所有。",
        "为建立连接，客户端可能修改系统代理、创建虚拟网络接口、安装或调用后台服务并申请必要权限。停止连接、退出账户或卸载前，请按客户端流程操作。为修复安全问题和保持兼容性，我们可能提供或要求安装更新。",
      ],
    },
    {
      title: "8. 第三方服务与内容",
      paragraphs: [
        "本服务仅提供网络传输能力，不控制您访问的第三方网站、应用、内容和交易。第三方可能根据其条款限制 VPN、代理、自动化访问或特定地区访问。您与第三方之间的争议应按照第三方规则及适用法律处理。",
      ],
    },
    {
      title: "9. 暂停与终止",
      paragraphs: [
        "当账户欠费、订阅到期、流量耗尽、存在安全风险、违反本协议、收到有效法律要求或为保护网络及其他用户所必需时，我们可以在合理范围内限制、暂停或终止服务。紧急情况下可能无法提前通知。",
        "您可以停止使用服务，并通过客户端提供的账户或客服渠道申请注销。终止不影响终止前已经产生的付款义务、责任及依法应继续有效的条款。",
      ],
    },
    {
      title: "10. 免责声明与责任限制",
      paragraphs: [
        "在适用法律允许的最大范围内，本服务按“现状”和“可用”状态提供。我们不对互联网本身、运营商、第三方服务、不可抗力或您设备配置造成的中断、速度变化、数据丢失和访问失败承担超出法定范围的责任。",
        "我们不排除或限制依法不得排除的消费者权利、人身损害责任或因故意、重大过失产生的责任。其他责任应根据直接损失、可预见性、双方过错及适用法律确定。",
      ],
    },
    {
      title: "11. 协议变更",
      paragraphs: [
        "我们可能因功能、法律、安全或运营变化更新本协议。重大变更将通过客户端、网站或其他合理方式提示，并标注新的生效日期。法律要求取得单独同意时，我们会在变更生效前请求您的同意。",
      ],
    },
    {
      title: "12. 适用法律、争议与联系",
      paragraphs: [
        "本协议适用客户端或官方网站公示的运营主体所在地法律，但不影响您所在地强制性消费者保护规定。发生争议时，请先通过客户端“在线客服”或“我的工单”联系我们协商；协商不成的，可向依法有管辖权的法院或争议解决机构寻求救济。",
        "运营主体名称、注册地址及正式联系方式以客户端、官方网站、订单凭证或客服页面的最新公示为准。",
      ],
    },
  ],
};

const PRIVACY: LegalDocumentContent = {
  eyebrow: "数据保护",
  title: "百夫长隐私VPN隐私政策",
  summary:
    "本政策说明百夫长隐私VPN在提供账户、订阅、VPN 连接、客户支持和软件更新时如何处理个人信息，以及您可以如何管理相关信息。",
  notice:
    "VPN 节点为了转发流量，在连接期间必然会处理网络地址、DNS 请求和数据包。我们不会将您的浏览内容出售给第三方，也不会将其用于跨站广告画像。",
  sections: [
    {
      title: "1. 适用范围与信息控制者",
      paragraphs: [
        "本政策适用于百夫长隐私VPN客户端、账户系统、节点服务、官方网站和客户支持。信息控制者或个人信息处理者为客户端及官方网站公示的实际运营主体。您通过本服务访问的第三方网站和应用由其自行处理信息，不受本政策约束。",
      ],
    },
    {
      title: "2. 我们处理的信息",
      bullets: [
        "账户信息：邮箱地址、用户编号、账户状态、验证记录以及用于身份校验的认证信息。",
        "订阅与交易信息：套餐、余额、流量用量、订单编号、金额、币种、优惠信息、支付状态和支付渠道返回的交易标识；我们通常不直接保存完整银行卡信息。",
        "设备与应用信息：操作系统、客户端版本、安装标识、语言、主题、启动设置、连接模式、代理端口、路由模式及必要的兼容性信息。",
        "连接与运行信息：连接状态、所选节点、协议类型、连接时间、汇总上传和下载流量、错误类别及必要的安全事件。客户端默认不保存完整浏览历史或通信内容。",
        "支持信息：您提交的工单、聊天内容、诊断描述、附件以及处理记录。请勿在支持请求中提交与问题无关的敏感信息。",
        "邀请和活动信息：邀请码、邀请关系、奖励、佣金、礼品卡状态以及防止欺诈所需的记录。",
      ],
    },
    {
      title: "3. VPN 流量的处理方式",
      paragraphs: [
        "建立 VPN 连接时，客户端和节点需要在会话期间处理源 IP、目标 IP 或域名、端口、协议、DNS 请求及加密或未加密的数据包，以完成路由、转发、故障排查和安全防护。目标服务仍可能通过登录状态、Cookie、设备指纹或您主动提供的信息识别您。",
        "客户端的数据平面日志默认关闭，并以汇总计数方式向您显示流量。我们不以出售浏览记录或投放跨站行为广告为目的检查通信内容。发生滥用、安全事件、服务故障或有效法律要求时，我们可能在必要、适度且有权限的范围内处理相关技术信息。",
      ],
    },
    {
      title: "4. 处理目的",
      bullets: [
        "创建和维护账户，验证身份，提供登录、订阅、节点连接、流量统计和客户支持。",
        "执行订单、支付、退款、礼品卡、邀请和佣金功能，并保存必要的财务记录。",
        "分配节点、选择路由、维护网络容量、诊断故障、阻止欺诈和滥用并保障服务安全。",
        "提供软件更新、重要服务通知、安全提醒和协议变更通知。",
        "履行法律义务、响应有效法律程序并维护我们、用户及公众的合法权益。",
      ],
    },
    {
      title: "5. 处理依据与您的选择",
      paragraphs: [
        "我们根据履行合同、取得同意、遵守法律义务和维护网络安全等合法权益处理信息。某项信息不是提供核心功能所必需时，我们会在适用法律要求下征求同意或提供关闭选项。",
        "拒绝必要权限或信息可能导致无法登录、购买或建立 VPN 连接。您可以关闭开机启动、断开连接、切换路由模式或停止使用相应功能，以减少相关处理。",
      ],
    },
    {
      title: "6. 本地存储与系统权限",
      paragraphs: [
        "客户端会在设备上保存主题、连接偏好、节点选择、订阅运行配置及其他必要状态。认证凭据应存放在操作系统提供的受保护存储中。VPN 节点配置可能包含连接凭据，客户端和后台服务会限制其访问范围。",
        "系统代理模式会修改当前用户的代理设置；TUN 模式可能创建虚拟网络接口并需要提升权限；开机启动功能会注册系统启动项。您可以在设置中调整相关功能，并可通过注销或卸载清理相应状态。",
      ],
    },
    {
      title: "7. 信息共享与受托处理",
      paragraphs: [
        "我们不会出售您的个人信息。为提供服务，我们可能向节点和云基础设施提供商、支付机构、邮件和验证码服务商、客户支持工具、安全服务商及专业顾问提供完成其职责所必需的信息。",
        "这些接收方应依据合同、适用法律及我们的指示处理信息。发生合并、重组或资产转让时，相关信息可能随业务转移，我们会要求承接方继续遵守适用的数据保护义务。",
      ],
    },
    {
      title: "8. 跨境传输",
      paragraphs: [
        "VPN 的功能决定了流量可能经过您选择的其他国家或地区，账户、支持及基础设施服务也可能由不同地区的供应商提供。因此，信息可能被传输至您所在地以外并适用不同的数据保护规则。我们会根据适用法律采取合同、安全或其他必要措施。",
      ],
    },
    {
      title: "9. 保存期限",
      paragraphs: [
        "我们仅在实现本政策所述目的、处理争议、保障安全或履行法律义务所需期限内保存信息。账户信息通常保存至账户注销后完成必要清算；订单和财务记录按照法定期限保存；支持记录和安全记录按照问题处理及风险控制所需期限保存。",
        "本地设置会保留到您清除应用数据或卸载客户端。备份中的信息可能在正常轮换周期结束后删除。无法与个人关联的汇总或匿名信息可被长期用于容量和可靠性分析。",
      ],
    },
    {
      title: "10. 信息安全",
      paragraphs: [
        "我们采取访问控制、传输加密、凭据保护、最小权限、软件签名和安全更新等合理措施保护信息。但任何网络、设备或存储方式都无法保证绝对安全。您也应使用强密码、保护设备并及时安装客户端和系统更新。",
        "发生可能对您权益造成重大影响的数据安全事件时，我们会按照适用法律进行评估、处置和通知。",
      ],
    },
    {
      title: "11. 您的权利",
      bullets: [
        "根据适用法律，您可以请求访问、更正、复制或删除个人信息，限制或反对特定处理，以及撤回基于同意的授权。",
        "您可以通过账户页面修改部分资料、移除活动会话，并通过“在线客服”或“我的工单”提交其他请求。",
        "为防止未经授权的访问，我们可能在处理请求前验证您的身份。某些信息因财务、反欺诈、安全或法律义务可能无法立即删除。",
      ],
    },
    {
      title: "12. 未成年人",
      paragraphs: [
        "本服务不面向未达到所在地法定年龄且未取得监护人同意的未成年人。若我们发现未经有效同意处理了未成年人的个人信息，将采取合理措施删除或限制相关信息。监护人可通过客服渠道联系我们。",
      ],
    },
    {
      title: "13. 政策更新与联系",
      paragraphs: [
        "我们可能因功能、供应商或法律变化更新本政策。重大变更将通过客户端、网站或其他合理方式提示，并更新生效日期；依法需要时会重新征求同意。",
        "如需行使权利、提出隐私投诉或了解运营主体及数据保护联系方式，请通过客户端“在线客服”或“我的工单”联系我们。正式运营主体名称、注册地址和联系信息以客户端、官方网站或订单凭证的最新公示为准。",
      ],
    },
  ],
};

const DOCUMENTS: Record<LegalDocumentId, LegalDocumentContent> = {
  terms_of_service: TERMS,
  privacy_policy: PRIVACY,
};

function documentFromSearch(value: string | null): LegalDocumentId {
  return value === "privacy_policy" ? "privacy_policy" : "terms_of_service";
}

export function LegalPage({ authenticated }: { authenticated: boolean }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const activeId = documentFromSearch(searchParams.get("document"));
  const document = DOCUMENTS[activeId];
  const publicReturnTo =
    searchParams.get("returnTo") === "register" ? "/register" : "/login";
  const backTo = authenticated ? "/settings" : publicReturnTo;

  const selectDocument = (next: LegalDocumentId) => {
    const updated = new URLSearchParams(searchParams);
    updated.set("document", next);
    setSearchParams(updated, { replace: true });
  };

  return (
    <main
      className={
        authenticated
          ? "management-page legal-page"
          : "legal-page legal-page-public"
      }
    >
      <header className="management-heading legal-heading">
        <div>
          <span>法律与隐私</span>
          <h2>服务条款</h2>
          <p>以下文档内置于客户端，无需连接外部网站即可查看。</p>
        </div>
        <Link className="secondary-action" to={backTo}>
          <ArrowLeft aria-hidden="true" />
          返回
        </Link>
      </header>

      <div className="legal-tabs" role="tablist" aria-label="法律文档">
        <button
          type="button"
          role="tab"
          aria-selected={activeId === "terms_of_service"}
          onClick={() => selectDocument("terms_of_service")}
        >
          <FileText aria-hidden="true" />
          用户协议
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeId === "privacy_policy"}
          onClick={() => selectDocument("privacy_policy")}
        >
          <ShieldCheck aria-hidden="true" />
          隐私政策
        </button>
      </div>

      <article className="legal-document" key={activeId}>
        <header className="legal-document-heading">
          <span>{document.eyebrow}</span>
          <h1>{document.title}</h1>
          <p className="legal-effective-date">
            生效日期：{EFFECTIVE_DATE} · 版本：1.0
          </p>
          <p className="legal-summary">{document.summary}</p>
        </header>

        <div className="legal-notice">
          <ShieldCheck aria-hidden="true" />
          <p>{document.notice}</p>
        </div>

        <div className="legal-sections">
          {document.sections.map((section) => (
            <section key={section.title}>
              <h2>{section.title}</h2>
              {section.paragraphs?.map((paragraph) => (
                <p key={paragraph}>{paragraph}</p>
              ))}
              {section.bullets !== undefined && (
                <ul>
                  {section.bullets.map((bullet) => (
                    <li key={bullet}>{bullet}</li>
                  ))}
                </ul>
              )}
            </section>
          ))}
        </div>

        <footer className="legal-document-footer">
          <strong>百夫长隐私VPN</strong>
          <span>文档版本 1.0 · {EFFECTIVE_DATE}</span>
        </footer>
      </article>
    </main>
  );
}
