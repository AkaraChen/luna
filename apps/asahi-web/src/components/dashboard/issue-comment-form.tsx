import { type FormEvent } from "react";
import { Paperclip, Send } from "lucide-react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { RichTextEditor } from "@/components/ui/rich-text-editor";
import { cn } from "@/lib/utils";

interface IssueCommentFormProps {
  className?: string;
  isSubmitting?: boolean;
  onChange: (html: string) => void;
  onSubmit: (body: string) => void;
  value: string;
}

export function IssueCommentForm({
  className,
  isSubmitting = false,
  onChange,
  onSubmit,
  value,
}: IssueCommentFormProps) {
  const submitComment = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const body = value.trim();
    if (!isBlankRichText(body)) onSubmit(body);
  };

  const disabled = isSubmitting || isBlankRichText(value);

  return (
    <form
      className={cn(
        "shrink-0 px-6 py-4",
        className,
      )}
      onSubmit={submitComment}
    >
      <Card>
        <CardHeader>
          <CardTitle>New comment</CardTitle>
        </CardHeader>
        <CardContent className="flex flex-col gap-2">
          <RichTextEditor content={value} onChange={onChange} variant="plain" />
          <div className="flex items-center justify-between gap-2">
            <span className="inline-flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
              <span
                aria-hidden
                className="inline-flex size-5 items-center justify-center rounded-full bg-muted text-[9.5px] font-medium text-foreground"
              >
                You
              </span>
              Add a follow-up
            </span>
            <div className="flex items-center gap-1">
              <button
                aria-label="Attach"
                className="asahi-press rounded-md p-1.5 text-muted-foreground [transition:background-color_180ms_var(--ease-out-strong),color_180ms_var(--ease-out-strong)] hover:bg-muted hover:text-foreground"
                type="button"
              >
                <Paperclip className="size-3.5" />
              </button>
              <button
                aria-label="Send"
                className="asahi-press inline-flex size-7 items-center justify-center rounded-full bg-foreground text-background [transition:background-color_180ms_var(--ease-out-strong),transform_140ms_var(--ease-out-strong)] hover:bg-foreground/90 disabled:opacity-40"
                disabled={disabled}
                type="submit"
              >
                <Send className="size-3.5" />
              </button>
            </div>
          </div>
        </CardContent>
      </Card>
    </form>
  );
}

function isBlankRichText(html: string) {
  return !html
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/\u00A0/g, " ")
    .trim();
}
