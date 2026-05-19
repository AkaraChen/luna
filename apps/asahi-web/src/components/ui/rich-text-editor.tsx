import { useEffect } from "react";
import { Tiptap, useEditor, useTiptap } from "@tiptap/react";
import { BubbleMenu } from "@tiptap/react/menus";
import StarterKit from "@tiptap/starter-kit";

import { cn } from "@/lib/utils";

interface RichTextEditorProps {
  content: string;
  onChange?: (html: string) => void;
  editable?: boolean;
  className?: string;
  placeholder?: string;
  variant?: "plain" | "bordered";
}

export function RichTextEditor({
  content,
  onChange,
  editable = true,
  className,
  variant = "plain",
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
          "[&_blockquote]:border-l [&_blockquote]:border-border [&_blockquote]:pl-3 [&_blockquote]:italic [&_blockquote]:text-muted-foreground",
          "[&_code]:bg-muted [&_code]:px-1 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-[12px]",
          "[&_pre]:bg-muted [&_pre]:p-3 [&_pre]:rounded-md [&_pre]:text-[12px] [&_pre]:font-mono",
          editable && "min-h-[4rem] cursor-text",
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
        className={cn(editorFrameClassName(editable, variant), "animate-pulse", className)}
      />
    );
  }

  return (
    <div className={cn(editorFrameClassName(editable, variant), className)}>
      <Tiptap instance={editor}>
        <Tiptap.Content />
        <EditorBubbleMenu />
      </Tiptap>
    </div>
  );
}

function editorFrameClassName(
  editable: boolean,
  variant: NonNullable<RichTextEditorProps["variant"]>,
) {
  return cn(
    "bg-background",
    variant === "bordered" &&
      editable &&
      "w-full rounded-xl border border-border/70 bg-muted/40 px-3 py-2.5 text-[13.5px] [transition:background-color_180ms_var(--ease-out-strong),border-color_180ms_var(--ease-out-strong)] focus-within:bg-background focus-within:border-foreground/40",
  );
}

function EditorBubbleMenu() {
  const { editor } = useTiptap();

  if (!editor) return null;

  return (
    <BubbleMenu editor={editor}>
      <div className="inline-flex items-center gap-0.5 rounded-lg border border-border/70 bg-popover px-1 py-0.5 shadow-[0_1px_2px_oklch(0_0_0_/_0.04)]">
        <MenuButton
          active={editor.isActive("bold")}
          label="Bold"
          onClick={() => editor.chain().focus().toggleBold().run()}
        >
          <span className="text-[11px] font-medium">B</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("italic")}
          label="Italic"
          onClick={() => editor.chain().focus().toggleItalic().run()}
        >
          <span className="text-[11px] italic">I</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("strike")}
          label="Strikethrough"
          onClick={() => editor.chain().focus().toggleStrike().run()}
        >
          <span className="text-[11px] line-through">S</span>
        </MenuButton>
        <div className="mx-0.5 h-4 w-px bg-border" />
        <MenuButton
          active={editor.isActive("heading", { level: 2 })}
          label="Heading 2"
          onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
        >
          <span className="text-[11px] font-medium">H2</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("heading", { level: 3 })}
          label="Heading 3"
          onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
        >
          <span className="text-[11px] font-medium">H3</span>
        </MenuButton>
        <div className="mx-0.5 h-4 w-px bg-border" />
        <MenuButton
          active={editor.isActive("bulletList")}
          label="Bullet list"
          onClick={() => editor.chain().focus().toggleBulletList().run()}
        >
          <span className="text-[11px]">•</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("orderedList")}
          label="Ordered list"
          onClick={() => editor.chain().focus().toggleOrderedList().run()}
        >
          <span className="text-[11px]">1.</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("blockquote")}
          label="Quote"
          onClick={() => editor.chain().focus().toggleBlockquote().run()}
        >
          <span className="text-[11px]">&ldquo;</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("codeBlock")}
          label="Code block"
          onClick={() => editor.chain().focus().toggleCodeBlock().run()}
        >
          <span className="font-mono text-[11px]">{`</>`}</span>
        </MenuButton>
      </div>
    </BubbleMenu>
  );
}

function MenuButton({
  active,
  children,
  label,
  onClick,
}: {
  active: boolean;
  children: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      aria-label={label}
      className={cn(
        "asahi-press flex size-7 items-center justify-center rounded text-muted-foreground [transition:background-color_180ms_var(--ease-out-strong),color_180ms_var(--ease-out-strong)] hover:bg-muted/60 hover:text-foreground",
        active && "bg-muted text-foreground",
      )}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}
