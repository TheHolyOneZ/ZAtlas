import { X } from "lucide-react";

import { RailLabel } from "./ui";
import { useRepoStore } from "../store/useRepoStore";


export default function RepoList() {
  const { roots, removeRepo } = useRepoStore();
  if (roots.length < 2) return null;

  return (
    <>
      <RailLabel>Workspace</RailLabel>
      <div className="pb-1">
        {roots.map((root) => {
          const name = root.split("/").filter(Boolean).pop() ?? root;
          return (
            <div
              key={root}
              className="group flex items-center gap-2 px-4 h-7 hover:bg-[var(--row-hover)]"
              title={root}
            >
              <span
                aria-hidden
                className="w-1 h-1 rounded-full shrink-0"
                style={{ background: "var(--accent)" }}
              />
              <span className="mono truncate text-[12px]" style={{ color: "var(--text-2)" }}>
                {name}
              </span>
              <button
                type="button"
                onClick={() => void removeRepo(root)}
                title={`Remove ${name} from the workspace`}
                aria-label={`Remove ${name} from the workspace`}
                className="ml-auto shrink-0 grid place-items-center w-4 h-4 rounded-[2px] opacity-0 group-hover:opacity-100 transition-opacity duration-100 hover:bg-[var(--control-hover)]"
                style={{ color: "var(--text-3)" }}
              >
                <X size={11} />
              </button>
            </div>
          );
        })}
      </div>
    </>
  );
}
