import * as React from "react";
import { useMemo, useEffect } from "react";
import { createSelector } from "reselect";
import { joinScenario, createScenario } from "lib/serverManager";
import { Logout } from "components/scenarios/Authentication";
import { MetaData } from "lib/entities";

import { RootState, store, resetState } from "state/store";
import { useAppSelector, useAppDispatch } from "state/hooks";
import { AppMode, setAppMode } from "state/tutorialSlice";

const TUTORIAL_SCENARIO = "tutorial.json";
export const TUTORIAL_PREFIX = "$TUTORIAL-";
export const SCENARIO_BUILDER_PREFIX = "SCENARIO-";

// This function will generate a three word name (with hyphens between the words) or random words
// to be used as the name for a new scenario.
function generateUniqueHyphenatedName() {
  // eslint-disable-next-line array-element-newline
  const words = [
    "ocean",
    "mountain",
    "river",
    "forest",
    "desert",
    "island",
    "valley",
    "canyon",
    "meadow",
    "glacier",
    "sunset",
    "sunrise",
    "twilight",
    "midnight",
    "shadow",
    "light",
    "storm",
    "breeze",
    "whisper",
    "silence",
    "echo",
    "journey",
    "voyage",
    "adventure",
    "discovery",
    "secret",
    "mystery",
    "wonder",
    "magic",
    "dream",
    "fantasy",
    "reality",
    "truth",
    "wisdom",
    "courage",
    "strength",
    "peace",
    "joy",
    "laughter",
    "tears",
    "heart",
    "soul",
    "spirit",
    "mind",
    "thought",
    "idea",
    "vision",
    "passion",
    "grace",
    "beauty",
    "charm",
    "elegance",
    "style",
    "fashion",
    "art",
    "music",
    "dance",
    "theater",
    "cinema",
    "story",
    "poem",
    "novel",
    "legend",
    "myth",
    "destiny",
    "fate",
    "chance",
    "luck",
    "hope",
    "faith",
    "charity",
    "kindness",
    "friendship",
    "love",
    "family",
    "home",
    "garden",
    "flower",
    "tree",
    "bird",
    "fish",
    "star",
    "planet",
    "galaxy",
    "universe",
    "time",
    "space",
    "matter",
    "energy",
    "motion",
    "change",
    "growth",
    "creation",
    "destruction",
    "balance",
    "harmony",
    "silence",
    "sound",
    "color",
    "shape",
    "texture",
    "taste",
    "smell",
    "touch",
    "feeling",
    "emotion",
    "desire",
    "belief",
    "knowledge",
    "learning",
  ];
  const selectedWords = [];
  const availableIndices = [...words.keys()];

  while (selectedWords.length < 3 && availableIndices.length > 0) {
    const randomIndex = Math.floor(Math.random() * availableIndices.length);
    const wordIndex = availableIndices.splice(randomIndex, 1)[0];
    selectedWords.push(words[wordIndex]);
  }

  return selectedWords.join("-");
}

type ScenarioManagerProps = unknown;
type ScenarioManagerScreen = "mode-select" | "game" | "builder";

export const ScenarioManager: React.FC<ScenarioManagerProps> = () => {
  const activeScenarios = useAppSelector(
    (state) => state.server.activeScenarios,
  );
  const scenarioTemplates = useAppSelector(
    (state) => state.server.scenarioTemplates,
  );
  const sortedSelector = createSelector(
    (state: RootState) => state.server.scenarioTemplates,
    (templates: [string, MetaData][]) => {
      return templates
        .slice()
        .sort((a: [string, MetaData], b: [string, MetaData]) =>
          a[1].name.localeCompare(b[1].name),
        );
    },
  );

  const sortedTemplates = useAppSelector(sortedSelector);

  const dispatch = useAppDispatch();

  const sortedFilteredScenarios = useMemo(() => {
    return activeScenarios
      .map((scenario) => scenario[0])
      .filter(
        (scenario) =>
          !scenario.startsWith(TUTORIAL_PREFIX) &&
          !scenario.startsWith(SCENARIO_BUILDER_PREFIX),
      )
      .sort((a, b) => a.localeCompare(b));
  }, [activeScenarios]);

  const builderTemplates = useMemo(
    () =>
      sortedTemplates.filter(
        (scenario: [string, MetaData]) => scenario[0] !== TUTORIAL_SCENARIO,
      ),
    [sortedTemplates],
  );

  const [scenario, setScenario] = React.useState<string | null>(
    sortedFilteredScenarios[0] ?? null,
  );
  const [template, setTemplate] = React.useState<string | null>(null);
  const [screen, setScreen] =
    React.useState<ScenarioManagerScreen>("mode-select");
  const [showScenarioIntro, setShowScenarioIntro] = React.useState<{
    name: string;
    description: string;
    handler: () => void;
  } | null>(null);

  // Initialize template to the first builder template if not already set
  useEffect(() => {
    if (!template && builderTemplates.length > 0) {
      setTemplate(builderTemplates[0][0]);
    }
  }, [builderTemplates, template]);

  const scenarioName = useMemo(() => generateUniqueHyphenatedName(), []);
  const builderScenarioName = useMemo(
    () => `${SCENARIO_BUILDER_PREFIX}${generateUniqueHyphenatedName()}`,
    [],
  );

  useEffect(() => {
    if (scenario == null) {
      setScenario(sortedFilteredScenarios[0] ?? null);
    }
  }, [scenario, sortedFilteredScenarios]);

  function handleTemplateSelectChange(
    event: React.ChangeEvent<HTMLSelectElement>,
  ) {
    setTemplate(event.target.value);
  }

  function handleScenarioSelectChange(
    event: React.ChangeEvent<HTMLSelectElement>,
  ) {
    setScenario(event.target.value);
  }

  function prepareScenarioLaunch(mode: AppMode) {
    store.dispatch(resetState());
    dispatch(setAppMode(mode));
  }

  function handleJoinScenario(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!scenario) {
      console.error(
        "(ScenarioManager.handleJoinScenario) No scenario selected",
      );
      return;
    }

    const scenarioName = (activeScenarios.find((s) => s[0] === scenario) ?? [
      "",
      "",
    ])[1];

    if (scenarioName === "") {
      joinScenario(scenario);
      return;
    }
    const scenarioData = scenarioTemplates.find((s) => s[0] === scenarioName);
    if (!scenarioData) {
      console.error(
        "(ScenarioManager.handleJoinScenario) No scenario data found for scenario " +
          scenario,
      );
      joinScenario(scenario);
      return;
    }

    setShowScenarioIntro({
      name: scenarioData[1].name,
      description: scenarioData[1].description,
      handler: () => {
        setShowScenarioIntro(null);
        prepareScenarioLaunch(AppMode.Game);
        joinScenario(scenario ?? "");
      },
    });
  }

  function handleCreateScenario(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (template === null) {
      prepareScenarioLaunch(AppMode.Game);
      createScenario(scenarioName, template ?? "");
      return;
    }

    const scenarioData = scenarioTemplates.find((s) => s[0] === template);

    if (!scenarioData) {
      console.error(
        "(ScenarioManager.handleCreateScenario) No scenario data found for scenario " +
          template,
      );
      createScenario(scenarioName, template ?? "");
      return;
    }

    setShowScenarioIntro({
      name: scenarioData[1].name,
      description: scenarioData[1].description,
      handler: () => {
        setShowScenarioIntro(null);
        prepareScenarioLaunch(AppMode.Game);
        createScenario(scenarioName, template ?? "");
      },
    });
  }

  function handleBuilderLoadScenario(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!template) {
      console.error(
        "(ScenarioManager.handleBuilderLoadScenario) No scenario selected",
      );
      return;
    }

    prepareScenarioLaunch(AppMode.ScenarioBuilder);
    createScenario(builderScenarioName, template);
  }

  function handleBuilderCreateScenario(
    event: React.FormEvent<HTMLFormElement>,
  ) {
    event.preventDefault();
    prepareScenarioLaunch(AppMode.ScenarioBuilder);
    createScenario(builderScenarioName, "");
  }

  function launchTutorial() {
    prepareScenarioLaunch(AppMode.Tutorial);
    const random_tutorial_name =
      TUTORIAL_PREFIX + generateUniqueHyphenatedName();
    createScenario(random_tutorial_name, TUTORIAL_SCENARIO);
  }

  function backToModeSelect() {
    setShowScenarioIntro(null);
    setScreen("mode-select");
  }

  return (
    <>
      {showScenarioIntro && (
        <ScenarioIntro
          scenarioName={showScenarioIntro.name}
          scenarioDescription={showScenarioIntro.description}
          handler={showScenarioIntro.handler}
        />
      )}
      {!showScenarioIntro && (
        <>
          <div className="authentication-container">
            {screen === "mode-select" && (
              <>
                <div className="authentication-blurb">
                  <h1 className="authentication-title">Callisto</h1>
                  <br />
                  <br />
                  From this screen you can create a scenario, join an existing
                  one to play with others, or just try the tutorial. Each
                  scenario has a unique three-word generated name.
                  <br />
                  <br />
                  Right now we show all scenarios to make Callisto easier to
                  use. Out of courtesy, please do not join a scenario to which
                  you haven&apos;t been invited!
                </div>
                <br />
                <br />
                <form
                  className="scenario-join-form"
                  onSubmit={handleJoinScenario}
                >
                  <h1>Join Existing Scenario</h1>
                  <br />
                  <select
                    className="select-dropdown control-name-input control-input"
                    name="scenario_selector"
                    value={scenario ?? ""}
                    onChange={handleScenarioSelectChange}
                  >
                    {sortedFilteredScenarios.length === 0 && (
                      <option value="">&lt;no scenarios&gt;</option>
                    )}
                    {sortedFilteredScenarios.map((scenario) => (
                      <option key={scenario} value={scenario}>
                        {scenario}
                      </option>
                    ))}
                  </select>
                  <button
                    className="control-input control-button blue-button"
                    type="submit"
                    disabled={!scenario}
                  >
                    Join
                  </button>
                </form>
                <br />
                <br />
                <form
                  className="scenario-create-form"
                  onSubmit={handleCreateScenario}
                >
                  <h1>Create New Scenario</h1>
                  <br />
                  <span className="label-scenario-name">
                    <b>Name:</b> {scenarioName}
                  </span>
                  <select
                    className="select-dropdown control-name-input control-input"
                    name="scenario_template_selector"
                    value={template ?? ""}
                    onChange={handleTemplateSelectChange}
                  >
                    <option key="default" value="">
                      &lt;no scenario&gt;
                    </option>
                    {sortedTemplates.map((scenario: [string, MetaData]) => (
                      <option key={scenario[1].name} value={scenario[0]}>
                        {scenario[1].name}
                      </option>
                    ))}
                  </select>
                  <button
                    className="control-input control-button blue-button"
                    type="submit"
                  >
                    Create
                  </button>
                </form>
                <br />
                <br />
                <div className="scenario-mode-buttons">
                  <button
                    className="blue-button tutorial-button"
                    onClick={launchTutorial}
                  >
                    Launch Tutorial
                  </button>
                  <button
                    className="blue-button tutorial-button"
                    onClick={() => setScreen("builder")}
                  >
                    Scenario Builder
                  </button>
                </div>
              </>
            )}
            {screen === "builder" && (
              <>
                <div className="authentication-blurb">
                  <h1 className="authentication-title">Scenario Builder</h1>
                  <br />
                  <br />
                  Load an existing scenario to modify it, or start a totally
                  blank builder session. Saving back to storage will come in a
                  later phase.
                </div>
                <br />
                <br />
                <form
                  className="scenario-join-form"
                  onSubmit={handleBuilderLoadScenario}
                >
                  <h1>Edit Existing Scenario</h1>
                  <br />
                  <select
                    className="select-dropdown control-name-input control-input"
                    name="builder_scenario_selector"
                    value={template ?? ""}
                    onChange={handleTemplateSelectChange}
                  >
                    {builderTemplates.length === 0 && (
                      <option value="">&lt;no scenarios&gt;</option>
                    )}
                    {builderTemplates.map((scenario: [string, MetaData]) => (
                      <option key={scenario[1].name} value={scenario[0]}>
                        {scenario[1].name}
                      </option>
                    ))}
                  </select>
                  <button
                    className="control-input control-button blue-button"
                    type="submit"
                    disabled={!template}
                  >
                    Open Builder
                  </button>
                </form>
                <br />
                <br />
                <form
                  className="scenario-create-form"
                  onSubmit={handleBuilderCreateScenario}
                >
                  <h1>Create Blank Builder Session</h1>
                  <br />
                  <button
                    className="control-input control-button blue-button"
                    type="submit"
                  >
                    Create Blank Scenario
                  </button>
                </form>
                <br />
                <br />
                <button
                  className="blue-button tutorial-button"
                  onClick={backToModeSelect}
                >
                  Back
                </button>
              </>
            )}
          </div>
          <div className="admin-button-window">
            <Logout />
          </div>
        </>
      )}
    </>
  );
};

type ScenarioIntroProps = {
  scenarioName: string;
  scenarioDescription: string;
  handler: () => void;
};

const ScenarioIntro: React.FC<ScenarioIntroProps> = ({
  scenarioName,
  scenarioDescription,
  handler,
}) => {
  return (
    <div className="authentication-container">
      <h1 className="authentication-title">{scenarioName}</h1>
      <br />
      <br />
      <div className="authentication-blurb">{scenarioDescription}</div>
      <br />
      <br />
      <button className="blue-button" onClick={handler}>
        Lets Go!
      </button>
    </div>
  );
};
