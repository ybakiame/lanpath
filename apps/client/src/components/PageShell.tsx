import type { ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { Button, IS_MOBILE, Navbar } from "@lanpath/ui";

export function PageShell({ children }: { children: ReactNode }) {
  return (
    <div
      className={
        IS_MOBILE
          ? "mx-auto flex min-h-full w-full max-w-2xl flex-col gap-5 pr-[max(1.25rem,env(safe-area-inset-right))] pl-[max(1.25rem,env(safe-area-inset-left))] pt-[calc(1.5rem+env(safe-area-inset-top))] pb-8"
          : "flex flex-col gap-6"
      }
    >
      {children}
    </div>
  );
}
export function SubPageShell({
  title,
  backTo,
  action,
  children,
}: {
  title: string;
  backTo?: string;
  action?: ReactNode;
  children: ReactNode;
}) {
  const navigate = useNavigate();
  const back = (
    <Button
      variant="ghost"
      isIconOnly
      aria-label="返回"
      onPress={() => (backTo ? navigate(backTo, { replace: true }) : navigate(-1))}
    >
      <ArrowLeft className="size-5" />
    </Button>
  );
  return (
    <div className="flex min-h-full flex-col gap-4">
      {IS_MOBILE ? (
        <Navbar title={title} left={back} right={action} />
      ) : (
        <header className="flex items-center gap-3">
          {back}
          <h1 className="flex-1 text-xl font-semibold">{title}</h1>
          {action}
        </header>
      )}
      <div
        className={
          IS_MOBILE
            ? "mx-auto flex w-full max-w-2xl flex-col gap-5 pr-[max(1.25rem,env(safe-area-inset-right))] pl-[max(1.25rem,env(safe-area-inset-left))] pb-[max(1.5rem,env(safe-area-inset-bottom))]"
            : "flex flex-col gap-4"
        }
      >
        {children}
      </div>
    </div>
  );
}
