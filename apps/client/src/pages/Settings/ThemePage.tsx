import { SubPageShell } from "../../components/PageShell";
import { AppearanceCard } from "./AppearanceCard";

export default function ThemePage() {
  return (
    <SubPageShell title="主题" backTo="/settings">
      <AppearanceCard />
    </SubPageShell>
  );
}
