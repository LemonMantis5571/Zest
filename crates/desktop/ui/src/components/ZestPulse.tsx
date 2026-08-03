import { BrandMark } from "@/components/BrandMark";
import { cn } from "@/lib/utils";

/** Soft lemon pulse while the agent is thinking, typing, or working. */
export function ZestPulse({
  size = 14,
  className,
}: {
  size?: number;
  className?: string;
}) {
  return (
    <span
      role="status"
      aria-label="Working"
      className={cn(
        "relative inline-grid shrink-0 place-items-center",
        className
      )}
      style={{ width: size, height: size }}
    >
      <span
        className="zest-pulse-ring pointer-events-none absolute inset-[-3px] rounded-full"
        aria-hidden
      />
      <BrandMark size={size} className="zest-pulse-mark relative" />
    </span>
  );
}
