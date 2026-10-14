import { useState } from "react";
import { PanelLeftOpen } from "lucide-react";
import { Button, Modal } from "@lanpath/ui";
import { DesktopSidebar } from "./DesktopSidebar";

/** 系统标题栏和自绘标题栏下均保留窄窗口导航入口。 */
export function CompactNavigation() {
  const [open, setOpen] = useState(false);
  return (
    <div className="shrink-0 border-b border-border bg-surface px-4 py-2 lg:hidden">
      <Button variant="ghost" aria-haspopup="dialog" aria-expanded={open} onPress={() => setOpen(true)}>
        <PanelLeftOpen className="size-5" aria-hidden="true" />
        展开导航
      </Button>
      <Modal.Backdrop isOpen={open} onOpenChange={setOpen}>
        <Modal.Container>
          <Modal.Dialog className="sm:max-w-sm">
            <Modal.Header>
              <Modal.Heading>主导航</Modal.Heading>
              <Modal.CloseTrigger />
            </Modal.Header>
            <Modal.Body>
              <DesktopSidebar compact onNavigate={() => setOpen(false)} />
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </div>
  );
}
