import { Button } from "@/components/ui/button";
import { CheckIcon } from "lucide-react";

type Props = {
  onContinue: () => void;
  continuing: boolean;
};

export function AuthSuccess({ onContinue, continuing }: Props) {
  return (
    <section className="w-full max-w-[420px]">
      <header className="mb-7">
        <div className="mb-4.5 flex size-12 items-center justify-center rounded-full bg-[var(--success)] text-white">
          <CheckIcon className="size-6" strokeWidth={3} />
        </div>
        <h1 className="m-0 mb-2 text-[28px] font-semibold leading-[1.2] tracking-[-0.6px]">
          Authentication successful
        </h1>
        <p className="m-0 max-w-[36ch] text-sm text-muted-foreground">
          You’re signed in. Continue in Zest — no need to return to a terminal.
        </p>
      </header>
      <footer className="mt-7 flex justify-end gap-2.5">
        <Button type="button" disabled={continuing} onClick={onContinue}>
          {continuing ? "Starting…" : "Continue"}
        </Button>
      </footer>
    </section>
  );
}
