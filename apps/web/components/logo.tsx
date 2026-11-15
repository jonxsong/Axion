import Link from "next/link";

export function Logo({ compact = false }: { compact?: boolean }) {
  return (
    <Link href="/" className="logo" aria-label="Axion home">
      <svg viewBox="0 0 36 36" role="img" aria-hidden="true">
        <path d="M5 28 16.2 6h4.1L31 28h-5.7l-2.2-4.9H13L10.7 28H5Z" fill="currentColor" />
        <path d="m15.1 18.5 3-6.7 3 6.7h-6Z" fill="var(--background)" />
        <circle cx="29" cy="7" r="3" fill="var(--accent)" />
      </svg>
      {!compact && <span>AXION</span>}
    </Link>
  );
}

