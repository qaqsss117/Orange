import { Globe } from "lucide-react";
import { regionCodeForTag } from "./nodeRegion";

// 国旗素材来自 flag-icons(https://github.com/lipis/flag-icons),MIT License。
const FLAG_URLS: Record<string, string> = Object.fromEntries(
  Object.entries(
    import.meta.glob("../../assets/flags/*.svg", {
      eager: true,
      query: "?url",
      import: "default",
    }) as Record<string, string>,
  ).map(([path, url]) => [
    path.slice(path.lastIndexOf("/") + 1, -".svg".length),
    url,
  ]),
);

export function NodeRegionIcon({ tag }: { tag: string | null }) {
  if (tag === null) {
    return null;
  }
  const code = regionCodeForTag(tag);
  const url = code === null ? undefined : FLAG_URLS[code];
  if (url === undefined) {
    return <Globe className="node-flag node-flag-fallback" aria-hidden="true" />;
  }
  return <img className="node-flag" src={url} alt="" aria-hidden="true" />;
}
