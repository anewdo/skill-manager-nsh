import { Banner, Button, Label, Modal, TextField } from "../../../components/ui";
import type { AppStore } from "../../../store";
import type { LibraryController } from "../controller";

/** The dialogs that confirm or complete a library action: rename, delete, overwrite. */
export function LibraryDialogs(props: { store: AppStore; controller: LibraryController }) {
  const text = props.store.text;
  const c = props.controller;
  return (
    <>
      <Modal
        open={c.renameTarget() !== null}
        size="sm"
        title={text("library.rename.title")}
        closeLabel={text("common.close")}
        onClose={c.closeRename}
        footer={
          <>
            <Button label={text("common.cancel")} variant="ghost" onPress={c.closeRename} />
            <Button
              label={text("library.rename.confirm")}
              variant="primary"
              onPress={c.submitRename}
              testID="library-rename-confirm"
            />
          </>
        }
      >
        <TextField
          value={c.renameValue()}
          placeholder={c.renameTarget()?.display_name ?? ""}
          onChange={c.editRename}
          onSubmit={c.submitRename}
          testID="library-rename-input"
        />
        <If when={c.renameError()}>
          {(message) => (
            <Label variant="caption" tone="danger">
              {message()}
            </Label>
          )}
        </If>
      </Modal>

      <Modal
        open={c.deleteOpen()}
        title={text("library.delete.title")}
        closeLabel={text("common.close")}
        onClose={c.closeDelete}
        footer={
          <>
            <Button label={text("common.cancel")} variant="ghost" onPress={c.closeDelete} />
            <Button
              label={text("library.delete.confirm")}
              variant="danger"
              onPress={c.confirmDelete}
              testID="library-delete-confirm"
            />
          </>
        }
      >
        <Label>
          {c.deleteItems().length === 1
            ? text("library.delete.single", { name: c.deleteItems()[0]?.display_name ?? "" })
            : text("library.delete.batch", { count: c.deleteItems().length })}
        </Label>
        <If when={c.deleteItems().some((skill) => skill.local_state === "modified")}>
          {() => (
            <Banner
              tone="warning"
              message={text("library.delete.modifiedWarning", {
                count: c.deleteItems().filter((skill) => skill.local_state === "modified").length,
              })}
            />
          )}
        </If>
      </Modal>

      <Modal
        open={c.overwriteTarget() !== null}
        title={text("library.update.confirm.title")}
        closeLabel={text("common.close")}
        onClose={c.closeOverwrite}
        footer={
          <>
            <Button label={text("common.cancel")} variant="ghost" onPress={c.closeOverwrite} />
            <Button
              label={text("library.update.confirm.action")}
              variant="danger"
              onPress={c.confirmOverwrite}
              testID="library-overwrite-confirm"
            />
          </>
        }
      >
        <Label>
          {text("library.update.confirm.message", {
            name: c.overwriteTarget()?.display_name ?? "",
          })}
        </Label>
      </Modal>
    </>
  );
}
