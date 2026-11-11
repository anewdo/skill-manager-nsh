import { Image, token, View } from "nsh";
import { Icon } from "../../../components/icon";
import { Button, Card, Label } from "../../../components/ui";
import type { MessageKey } from "../../../i18n";
import { formatCount } from "../../../lib/format";
import type { MarketplaceSkill } from "../../../lib/types";
import type { TextFn } from "../../../store";
import type { EntryStatus } from "../model";

const statusKeys: Record<EntryStatus, MessageKey> = {
  available: "marketplace.card.install",
  queued: "marketplace.card.installQueued",
  installing: "marketplace.card.installing",
  installed: "marketplace.card.installed",
};

export interface InstallButtonProps {
  status: EntryStatus;
  text: TextFn;
  onPress: () => void;
  testID?: string;
}

/** The button that installs an entry, and says where the install is once started. */
export function InstallButton(props: InstallButtonProps) {
  return (
    <Button
      label={props.text(statusKeys[props.status])}
      icon={props.status === "installed" ? "check" : "download"}
      variant={props.status === "installed" ? "ghost" : "primary"}
      size="sm"
      busy={props.status === "queued" || props.status === "installing"}
      disabled={props.status !== "available"}
      onPress={props.onPress}
      testID={props.testID}
    />
  );
}

/** The circular box the owner avatar and its placeholder share. */
const avatarShape = {
  width: 32,
  height: 32,
  borderRadius: 16,
  backgroundColor: token("muted"),
};

export interface MarketCardProps {
  entry: MarketplaceSkill;
  status: EntryStatus;
  /** Whether the entry has an owner, so the card always holds the avatar's place. */
  hasAvatar: boolean;
  /** The owner's GitHub avatar, null once its image failed: a placeholder takes its place. */
  avatarUrl: string | null;
  text: TextFn;
  onAvatarError: () => void;
  onPreview: (entry: MarketplaceSkill) => void;
  onInstall: (entry: MarketplaceSkill) => void;
}

/** One Marketplace Entry of the grid. */
export function MarketCard(props: MarketCardProps) {
  return (
    <Card class="flex-1 gap-3 p-4" testID={`market-entry-${props.entry.name}`}>
      <View class="flex-row items-center gap-2.5">
        <If when={props.hasAvatar}>
          <If when={props.avatarUrl} else={<View style={avatarShape} testID={`market-avatar-placeholder-${props.entry.name}`} />}>
            {(url) => (
              <Image
                src={url()}
                style={avatarShape}
                onError={() => props.onAvatarError()}
                testID={`market-avatar-${props.entry.name}`}
              />
            )}
          </If>
        </If>
        <View class="flex-1 gap-1">
          <Label variant="strong" lines={1}>
            {props.entry.name}
          </Label>
          <Label variant="caption" tone="muted" lines={1}>
            {props.entry.source}
          </Label>
        </View>
      </View>
      <View class="flex-row items-center gap-2">
        <View class="flex-1 flex-row items-center gap-1">
          <Icon name="download" size={12} />
          <Label variant="caption" tone="muted">
            {formatCount(props.entry.installs)}
          </Label>
        </View>
        <Button
          label={props.text("marketplace.card.preview")}
          icon="preview"
          variant="ghost"
          size="sm"
          onPress={() => props.onPreview(props.entry)}
          testID={`market-preview-${props.entry.name}`}
        />
        <InstallButton
          status={props.status}
          text={props.text}
          onPress={() => props.onInstall(props.entry)}
          testID={`market-install-${props.entry.name}`}
        />
      </View>
    </Card>
  );
}
