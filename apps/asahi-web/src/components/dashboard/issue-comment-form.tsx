import { type FormEvent } from "react";
import { IconSend } from "@tabler/icons-react";

import { Button } from "@/components/ui/button";
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

    if (!isBlankRichText(body)) {
      onSubmit(body);
    }
  };

  const disabled = isSubmitting || isBlankRichText(value);

  return (
    <form className={cn("p-4 pt-0", className)} onSubmit={submitComment}>
      <RichTextEditor content={value} onChange={onChange} />
      <div className="mt-2 flex justify-end">
        <Button disabled={disabled} size="sm" type="submit">
          <IconSend className="size-4" />
          Send
        </Button>
      </div>
    </form>
  );
}

function isBlankRichText(html: string) {
  return !html
    .replace(/<[^>]*>/g, "")
    .replace(/&nbsp;/g, " ")
    .replace(/\u00a0/g, " ")
    .trim();
}
