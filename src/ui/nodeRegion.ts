/**
 * 节点地区标签解析与映射。
 *
 * Xboard 的 Orange 订阅协议会把节点的第一个标签拼进节点名,
 * 形如 "[香港] 节点名称"。这里负责拆出标签,并把标签关键词
 * 映射为 ISO 3166-1 alpha-2 地区代码(小写,对应 assets/flags 文件名)。
 */

const TAG_PREFIX_PATTERN = /^\[([^[\]]{1,32})\]\s*/u;

export interface ParsedNodeName {
  tag: string | null;
  displayName: string;
}

export function parseNodeName(name: string): ParsedNodeName {
  const match = TAG_PREFIX_PATTERN.exec(name);
  const tag = match?.[1]?.trim();
  if (match === null || tag === undefined) {
    return { tag: null, displayName: name };
  }
  const displayName = name.slice(match[0].length).trim();
  return {
    tag,
    displayName: displayName === "" ? name : displayName,
  };
}

const CJK_PATTERN = /[⺀-鿿豈-﫿]/u;

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * 关键词表:[地区代码, 关键词...]。
 * 中文关键词按子串匹配;拉丁关键词按“前缀边界”匹配(允许后接数字,
 * 如 "HK01"),避免 "us" 误中 "russia" 这类情况。
 */
const REGION_KEYWORDS: ReadonlyArray<readonly [string, ...string[]]> = [
  ["hk", "香港", "hong kong", "hk"],
  ["mo", "澳门", "macau", "macao", "mo"],
  ["tw", "台湾", "taiwan", "tw"],
  ["cn", "中国", "大陆", "china", "cn"],
  ["jp", "日本", "japan", "jp"],
  ["kr", "韩国", "南韩", "korea", "kr"],
  ["sg", "新加坡", "狮城", "singapore", "sg"],
  ["us", "美国", "united states", "america", "usa", "us"],
  ["gb", "英国", "united kingdom", "britain", "england", "uk", "gb"],
  ["de", "德国", "germany", "de"],
  ["fr", "法国", "france", "fr"],
  ["ca", "加拿大", "canada", "ca"],
  ["au", "澳大利亚", "澳洲", "australia", "au"],
  ["nz", "新西兰", "new zealand", "nz"],
  ["my", "马来西亚", "malaysia", "my"],
  ["th", "泰国", "thailand", "th"],
  ["vn", "越南", "vietnam", "vn"],
  ["ph", "菲律宾", "philippines", "ph"],
  ["id", "印尼", "印度尼西亚", "indonesia", "id"],
  ["in", "印度", "india", "in"],
  ["tr", "土耳其", "turkey", "turkiye", "tr"],
  ["ru", "俄罗斯", "russia", "ru"],
  ["nl", "荷兰", "netherlands", "holland", "nl"],
  ["ch", "瑞士", "switzerland", "ch"],
  ["se", "瑞典", "sweden", "se"],
  ["no", "挪威", "norway", "no"],
  ["fi", "芬兰", "finland", "fi"],
  ["dk", "丹麦", "denmark", "dk"],
  ["pl", "波兰", "poland", "pl"],
  ["it", "意大利", "italy", "it"],
  ["es", "西班牙", "spain", "es"],
  ["pt", "葡萄牙", "portugal", "pt"],
  ["ae", "阿联酋", "迪拜", "united arab emirates", "dubai", "uae", "ae"],
  ["il", "以色列", "israel", "il"],
  ["ua", "乌克兰", "ukraine", "ua"],
  ["mx", "墨西哥", "mexico", "mx"],
  ["br", "巴西", "brazil", "br"],
  ["ar", "阿根廷", "argentina", "ar"],
  ["za", "南非", "south africa", "za"],
  ["eg", "埃及", "egypt", "eg"],
  ["ie", "爱尔兰", "ireland", "ie"],
  ["be", "比利时", "belgium", "be"],
  ["at", "奥地利", "austria", "at"],
  ["cz", "捷克", "czech", "cz"],
  ["hu", "匈牙利", "hungary", "hu"],
  ["ro", "罗马尼亚", "romania", "ro"],
  ["gr", "希腊", "greece", "gr"],
];

const REGION_MATCHERS: ReadonlyArray<readonly [string, RegExp[]]> =
  REGION_KEYWORDS.map(([code, ...keywords]) => [
    code,
    keywords.map((keyword) =>
      CJK_PATTERN.test(keyword)
        ? new RegExp(escapeRegExp(keyword), "iu")
        : new RegExp(`(^|[^a-z])${escapeRegExp(keyword)}(?![a-z])`, "iu"),
    ),
  ]);

export function regionCodeForTag(tag: string): string | null {
  for (const [code, matchers] of REGION_MATCHERS) {
    if (matchers.some((matcher) => matcher.test(tag))) {
      return code;
    }
  }
  return null;
}
