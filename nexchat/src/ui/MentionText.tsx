// EN: Render text with `@token` spans highlighted. CN: 高亮渲染 `@token` 片段。

const TOKEN_RE = /(@[A-Za-z0-9_-]+)/g;

export function MentionText({ text }: { text: string }) {
  const parts = text.split(TOKEN_RE);
  return (
    <>
      {parts.map((p, i) =>
        p.startsWith("@") ? (
          <span key={i} className="mention">
            {p}
          </span>
        ) : (
          <span key={i}>{p}</span>
        ),
      )}
    </>
  );
}
