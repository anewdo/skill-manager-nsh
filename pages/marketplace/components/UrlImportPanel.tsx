import { View } from "nsh";
import { EmptyState } from "../../../components/ui";
import type { AppStore } from "../../../store";

export function UrlImportPanel(props: { store: AppStore }) {
  const text = props.store.text;
  return (
    <View class="flex-1 pt-3" testID="import-url-unavailable">
      <EmptyState
        icon="globe"
        title={text("import.url.unavailable")}
        hint={text("import.url.unavailableHint")}
      />
    </View>
  );
}
