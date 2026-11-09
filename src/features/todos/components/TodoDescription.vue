<script setup lang="ts">
import { computed } from "vue";
import DOMPurify from "dompurify";
import { Marked } from "marked";
import { useToast } from "@nuxt/ui/composables";
import { useTodosStore } from "../stores/todosStore";
import type { AppError } from "@/types";

/** A todo description rendered as Markdown (GitHub flavor, single newlines kept). */
const props = defineProps<{ markdown: string }>();

const store = useTodosStore();
const toast = useToast();

function escape(text: string): string {
  return text.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
}

/** Link schemes that are kept and opened; mirrors `LINK_SCHEMES` in `services/todos.rs`. */
const LINK_SCHEMES = ["http", "https", "mailto", "obsidian"];
const ALLOWED_URI = new RegExp(`^(?:${LINK_SCHEMES.join("|")}):`, "i");

/** A bare app link such as `obsidian://open?vault=…`, minus trailing punctuation. */
const APP_LINK = /^obsidian:\/\/[^\s<>]*[^\s<>.,;:!?'")\]]/;

/** Shows percent-encoded app links readably, e.g. `%20` as a space. */
function readable(href: string): string {
  try {
    return decodeURI(href);
  } catch {
    return href;
  }
}

const marked = new Marked({
  gfm: true,
  breaks: true,
  // GitHub-style autolinking only covers `http(s)://` and `www.`; link bare app URLs too.
  extensions: [
    {
      name: "appLink",
      level: "inline",
      start: (src) => src.match(/obsidian:\/\//)?.index,
      tokenizer(src) {
        const match = APP_LINK.exec(src);
        if (match) return { type: "appLink", raw: match[0], href: match[0] };
      },
      renderer: (token) => `<a href="${escape(token.href)}">${escape(readable(token.href))}</a>`,
    },
  ],
  renderer: {
    // The CSP blocks remote images, so show them as links instead.
    image: ({ href, text }) => `<a href="${escape(href)}">${escape(text || href)}</a>`,
  },
});

// The description is the user's own text, but it is still sanitized: the webview can reach IPC.
const html = computed(() =>
  DOMPurify.sanitize(marked.parse(props.markdown, { async: false }), {
    FORBID_TAGS: ["img", "style", "form"],
    ALLOWED_URI_REGEXP: ALLOWED_URI,
  }),
);

/** Links open in the default browser instead of navigating the app's window. */
async function onClick(event: MouseEvent): Promise<void> {
  const link = (event.target as Element).closest("a");
  if (!link) return;
  event.preventDefault();
  const href = link.getAttribute("href");
  if (!href || href.startsWith("#")) return;
  try {
    await store.openLink(href);
  } catch (err: unknown) {
    toast.add({ title: (err as AppError).message, color: "error", icon: "i-lucide-circle-alert" });
  }
}
</script>

<template>
  <div
    class="break-words text-xs text-muted [&_a]:text-primary [&_a]:underline [&_a]:underline-offset-2 [&_blockquote]:border-l-2 [&_blockquote]:border-default [&_blockquote]:pl-2 [&_code]:rounded [&_code]:bg-elevated [&_code]:px-1 [&_code]:font-mono [&_h1]:font-semibold [&_h2]:font-semibold [&_h3]:font-semibold [&_h4]:font-semibold [&_hr]:border-default [&_li]:my-0.5 [&_ol]:list-decimal [&_ol]:pl-5 [&_pre]:overflow-x-auto [&_pre]:rounded [&_pre]:bg-elevated [&_pre]:p-2 [&_pre_code]:bg-transparent [&_pre_code]:p-0 [&_table]:border-collapse [&_td]:border [&_td]:border-default [&_td]:px-1.5 [&_th]:border [&_th]:border-default [&_th]:px-1.5 [&_ul]:list-disc [&_ul]:pl-5 [&_ul:has(input)]:list-none [&_ul:has(input)]:pl-1 [&>*+*]:mt-1.5"
    @click="onClick"
    v-html="html"
  />
</template>
