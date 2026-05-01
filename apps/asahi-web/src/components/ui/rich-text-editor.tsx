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
    <div className={cn("bg-background", className)}>
      <Tiptap instance={editor}>
        <div className="px-3 py-2">
          <Tiptap.Content />
        </div>
        <EditorBubbleMenu />
      </Tiptap>
    </div>
  );
}

function EditorBubbleMenu() {
  const { editor } = useTiptap();

  if (!editor) return null;

  return (
    <BubbleMenu editor={editor}>
      <div className="inline-flex items-center gap-0.5 rounded-lg border border-[#eceae5] bg-white px-1 py-0.5 shadow-lg">
        <MenuButton
          active={editor.isActive("bold")}
          label="Bold"
          onClick={() => editor.chain().focus().toggleBold().run()}
        >
          <span className="text-xs font-bold">B</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("italic")}
          label="Italic"
          onClick={() => editor.chain().focus().toggleItalic().run()}
        >
          <span className="text-xs italic">I</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("strike")}
          label="Strikethrough"
          onClick={() => editor.chain().focus().toggleStrike().run()}
        >
          <span className="text-xs line-through">S</span>
        </MenuButton>
        <div className="mx-0.5 h-4 w-px bg-[#eceae5]" />
        <MenuButton
          active={editor.isActive("heading", { level: 2 })}
          label="Heading 2"
          onClick={() => editor.chain().focus().toggleHeading({ level: 2 }).run()}
        >
          <span className="text-xs font-bold">H2</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("heading", { level: 3 })}
          label="Heading 3"
          onClick={() => editor.chain().focus().toggleHeading({ level: 3 }).run()}
        >
          <span className="text-xs font-bold">H3</span>
        </MenuButton>
        <div className="mx-0.5 h-4 w-px bg-[#eceae5]" />
        <MenuButton
          active={editor.isActive("bulletList")}
          label="Bullet list"
          onClick={() => editor.chain().focus().toggleBulletList().run()}
        >
          <span className="text-xs">•</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("orderedList")}
          label="Ordered list"
          onClick={() => editor.chain().focus().toggleOrderedList().run()}
        >
          <span className="text-xs">1.</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("blockquote")}
          label="Quote"
          onClick={() => editor.chain().focus().toggleBlockquote().run()}
        >
          <span className="text-xs">"</span>
        </MenuButton>
        <MenuButton
          active={editor.isActive("codeBlock")}
          label="Code block"
          onClick={() => editor.chain().focus().toggleCodeBlock().run()}
        >
          <span className="text-xs font-mono">{`</>`}</span>
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
        "flex size-7 items-center justify-center rounded text-[#55524b] transition-colors",
        active ? "bg-[#f2f1ec]" : "hover:bg-[#f7f6f2]",
      )}
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  );
}
