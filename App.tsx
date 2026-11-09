import { mount, signal, Text, View } from "nsh";

function App() {
  const count = signal(0);
  return (
    <View class="flex-1 pt-[env(safe-area-inset-top)] pr-[env(safe-area-inset-right)] pb-[env(safe-area-inset-bottom)] pl-[env(safe-area-inset-left)] items-center justify-center gap-4 bg-background">
      <Text class="text-3xl font-semibold text-foreground">{count()}</Text>
      <View
        onPress={() => count.update((c) => c + 1)}
        role="button"
        class="rounded-(--radius) bg-primary px-5 py-3 active:bg-primary/90"
      >
        <Text class="text-lg text-primary-foreground">Tap</Text>
      </View>
    </View>
  );
}

mount(App);
