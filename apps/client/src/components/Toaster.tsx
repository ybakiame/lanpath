import { useToastStore } from "@lanpath/client-core";
import { ToastRegion, IS_MOBILE } from "@lanpath/ui";

export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismissToast = useToastStore((s) => s.dismissToast);
  return <ToastRegion toasts={IS_MOBILE ? toasts.slice(-1) : toasts} onDismiss={dismissToast} />;
}
