export function BrandMark({ size = 28 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 28 28" fill="none" aria-hidden="true">
      <circle cx="14" cy="14" r="13" stroke="#5e6ad2" strokeWidth="1.5" />
      <path
        d="M9 15.5c1.2 2.2 3 3.5 5 3.5s3.8-1.3 5-3.5"
        stroke="#5e6ad2"
        strokeWidth="1.5"
        strokeLinecap="round"
      />
      <circle cx="10.5" cy="11" r="1.2" fill="#f7f8f8" />
      <circle cx="17.5" cy="11" r="1.2" fill="#f7f8f8" />
    </svg>
  );
}
