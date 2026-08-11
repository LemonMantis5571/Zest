import { cn } from "@/lib/utils";

const DOT_COUNT = 16;

/** Compact dotted activity mark while the agent is thinking, typing, or working. */
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
        "zest-pulse-dots relative inline-grid shrink-0 grid-cols-4 grid-rows-4 gap-px",
        className
      )}
      style={{ width: size, height: size }}
    >
      {Array.from({ length: DOT_COUNT }, (_, index) => (
        <span
          key={index}
          className="zest-pulse-dot aspect-square rounded-full"
          style={{ animationDelay: `${index * 45}ms` }}
          aria-hidden
        />
      ))}
    </span>
  );
}
