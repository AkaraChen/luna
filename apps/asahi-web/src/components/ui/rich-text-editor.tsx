import { useEffect } from "react";
import { Tiptap, useEditor } from "@tiptap/react";
import StarterKit from "@tiptap/starter-kit";

import { cn } from "@/lib/utils";

interface RichTextEditorProps {
  content: string;
  onChange?: (html: string) => void;
  editable?: boolean;
  className?: string;
  placeholder?: string;
}

export function RichTextEditor({
  content,
  onChange,
  editable = true,
  className,
}: RichTextEditorProps) {
  const editor = useEditor({
    extensions: [StarterKit],
    content,
    editable,
    immediatelyRender: false,
    editorProps: {
      attributes: {
        class: cn(
          "prose prose-sm max-w-none outline-none",
          "[&_p]:my-1.5 [&_h1]:my-2 [&_h2]:my-2 [&_h3]:my-2 [&_ul]:my-1.5 [&_ol]:my-1.5",
          "[&_blockquote]:border-l-2 [&_blockquote]:border-[#c9c4bb] [&_blockquote]:pl-3 [&_blockquote]:italic",
          "[&_code]:bg-[#f2f1ec] [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-xs",
          "[&_pre]:bg-[#f2f1ec] [&_pre]:p-3 [&_pre]:rounded-md [&_pre]:text-xs",
          editable && "min-h-[6rem] cursor-text",
        ),
      },
    },
  });

  useEffect(() => {
    if (!editor) return;
    const handler = () => {
      onChange?.(editor.getHTML());
    };
    editor.on("update", handler);
    return () => {
      editor.off("update", handler);
    };
  }, [editor, onChange]);

  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    if (editor.getHTML() !== content) {
      editor.commands.setContent(content, { emitUpdate: false });
    }
  }, [editor, content]);

  if (!editor) {
    return (
      <div
        className={cn(
          "min-h-[6rem] animate-pulse rounded-md bg-muted",
          className,
        )}
      />
    );
  }

  return (
    <div
      className={cn(
        "rounded-md border border-[#eceae5] bg-background",
        editable && "focus-within:border-[#c9c4bb] focus-within:ring-1 focus-within:ring-[#c9c4bb]/30",
        className,
      )}
    >
      <Tiptap instance={editor}>
        <div className="px-3 py-2">
          <Tiptap.Content />
        </div>
      </Tiptap>
    </div>
  );
}
