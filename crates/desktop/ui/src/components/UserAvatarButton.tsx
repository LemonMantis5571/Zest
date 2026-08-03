import { UserIcon } from "lucide-react";

import { cn } from "@/lib/utils";

type Props = {
  avatarDataUrl?: string;
  displayName?: string;
  title?: string;
  onClick: () => void;
  className?: string;
};

export function UserAvatarButton({
  avatarDataUrl,
  displayName,
  title = "User settings",
  onClick,
  className,
}: Props) {
  const initial = displayName?.trim()?.charAt(0)?.toUpperCase() ?? "";

  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      className={cn(
        "grid size-7 cursor-pointer place-items-center overflow-hidden rounded-md bg-card ring-1 ring-border outline-none transition-colors",
        "hover:ring-primary/50 focus-visible:ring-2 focus-visible:ring-ring/50",
        className
      )}
    >
      {avatarDataUrl ? (
        <img src={avatarDataUrl} alt="" className="size-full object-cover" />
      ) : initial ? (
        <span className="text-[12px] font-semibold text-foreground">{initial}</span>
      ) : (
        <UserIcon className="size-3.5 text-muted-foreground" />
      )}
    </button>
  );
}
