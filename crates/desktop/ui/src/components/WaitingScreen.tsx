import { BrandMark } from "@/components/BrandMark";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";

type Props = {
  title: string;
  body: string;
  hint: string;
  error: string | null;
  onCancel: () => void;
};

export function WaitingScreen({ title, body, hint, error, onCancel }: Props) {
  return (
    <section className="w-full max-w-[420px]">
      <header className="mb-7">
        <div className="mb-4.5">
          <BrandMark />
        </div>
        <h1 className="m-0 mb-2 text-[28px] font-semibold leading-[1.2] tracking-[-0.6px]">
          {title}
        </h1>
        <p className="m-0 max-w-[36ch] text-sm text-muted-foreground">{body}</p>
      </header>

      <div className="mt-2 flex items-center gap-3 rounded-xl border border-border bg-card px-4 py-3.5 text-[13px] text-[var(--ink-muted,#d0d6e0)]">
        <Spinner className="size-3.5 text-primary" />
        <span>{hint}</span>
      </div>

      {error ? <p className="mt-3.5 text-xs text-destructive">{error}</p> : null}

      <footer className="mt-7 flex justify-end gap-2.5">
        <Button type="button" variant="outline" onClick={onCancel}>
          Cancel
        </Button>
      </footer>
    </section>
  );
}
