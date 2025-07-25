import * as React from "react";
import {useMemo, useEffect} from "react";
import {createSelector} from "reselect";
import {joinScenario, createScenario} from "lib/serverManager";
import {Logout} from "components/scenarios/Authentication";
import {MetaData} from "lib/entities";

import {RootState, store, resetState} from "state/store";
import {useAppSelector, useAppDispatch} from "state/hooks";
import {setTutorialMode} from "state/tutorialSlice";

const TUTORIAL_SCENARIO = "tutorial.json";
export const TUTORIAL_PREFIX = "$TUTORIAL-";

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

export const ScenarioManager: React.FC<ScenarioManagerProps> = () => {
  const activeScenarios = useAppSelector((state) => state.server.activeScenarios);
  const scenarioTemplates = useAppSelector((state) => state.server.scenarioTemplates);
  const sortedSelector = createSelector((state: RootState) => state.server.scenarioTemplates, (templates: [string, MetaData][]) => {
    return templates.slice().sort((a: [string, MetaData], b: [string, MetaData]) => a[1].name.localeCompare(b[1].name));
  });

  const sortedTemplates = useAppSelector(sortedSelector);

  const dispatch = useAppDispatch();

  const sortedFilteredScenarios = useMemo(() => {
    return activeScenarios
      .map((scenario) => scenario[0])
      .filter((scenario) => !scenario.startsWith(TUTORIAL_PREFIX))
      .sort((a, b) => a.localeCompare(b));
  }, [activeScenarios]);

  const [scenario, setScenario] = React.useState<string | null>(sortedFilteredScenarios[0] ?? null);
  const [template, setTemplate] = React.useState<string | null>(null);
  const [showScenarioIntro, setShowScenarioIntro] = React.useState<{
    name: string;
    description: string;
    handler: () => void;
  } | null>(null);

  const scenarioName = generateUniqueHyphenatedName();

  useEffect(() => {
    if (scenario == null) {
      setScenario(sortedFilteredScenarios[0] ?? null);
    }
  }, [scenario, sortedFilteredScenarios]);

  function handleTemplateSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
    setTemplate(event.target.value);
  }

  function handleScenarioSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
    setScenario(event.target.value);
  }

  function handleJoinScenario(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    store.dispatch(resetState());
    if (!scenario) {
      console.error("(ScenarioManager.handleJoinScenario) No scenario selected");
      return;
    }

    const scenarioName = (activeScenarios.find((s) => s[0] === scenario) ?? ["", ""])[1];

    if (scenarioName === "") {
      joinScenario(scenario);
      return;
    }
    const scenarioData = scenarioTemplates.find((s) => s[0] === scenarioName);
    if (!scenarioData) {
      console.error(
        "(ScenarioManager.handleJoinScenario) No scenario data found for scenario " + scenario
      );
      joinScenario(scenario);
      return;
    }

    setShowScenarioIntro({
      name: scenarioData[1].name,
      description: scenarioData[1].description,
      handler: () => {
        setShowScenarioIntro(null);
        joinScenario(scenario ?? "");
      },
    });
  }

  function handleCreateScenario(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    store.dispatch(resetState());
    if (template === null) {
      createScenario(scenarioName, template ?? "");
      return;
    }

    const scenarioData = scenarioTemplates.find((s) => s[0] === template);

    if (!scenarioData) {
      console.error(
        "(ScenarioManager.handleCreateScenario) No scenario data found for scenario " + template
      );
      createScenario(scenarioName, template ?? "");
      return;
    }

    setShowScenarioIntro({
      name: scenarioData[1].name,
      description: scenarioData[1].description,
      handler: () => {
        setShowScenarioIntro(null);
        createScenario(scenarioName, template ?? "");
      },
    });
  }

  function launchTutorial() {
    dispatch(setTutorialMode(true));

    store.dispatch(resetState());
    const random_tutorial_name = TUTORIAL_PREFIX + generateUniqueHyphenatedName();
    createScenario(random_tutorial_name, TUTORIAL_SCENARIO);
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
            <div className="authentication-blurb">
              <h1 className="authentication-title">Scenarios</h1>
              <br />
              <br />
              From this screen you can create a scenario, join an existing one to play with others,
              or just try the tutorial. Each scenario has a unique three-word generated name.
              <br />
              Right now we show all scenarios to make Callisto easier to use. Out of courtesy,
              please do not join a scenario to which you haven&apos;t been invited!
              <br />
              <br />
              Note that if you customize a scenario from a template, after 5 minutes without any
              users logged in the scenario will be deleted.
            </div>
            <br />
            <br />
            <form className="scenario-join-form" onSubmit={handleJoinScenario}>
              <h1>Join Existing Scenario</h1>
              <br />
              <select
                className="select-dropdown control-name-input control-input"
                name="scenario_selector"
                value={scenario ?? ""}
                onChange={handleScenarioSelectChange}>
                {sortedFilteredScenarios.map((scenario) => (
                  <option key={scenario} value={scenario}>
                    {scenario}
                  </option>
                ))}
              </select>
              <button className="control-input control-button blue-button" type="submit">
                Join
              </button>
            </form>
            <br />
            <br />
            <form className="scenario-create-form" onSubmit={handleCreateScenario}>
              <h1>Create New Scenario</h1>
              <br />
              <span className="label-scenario-name">
                <b>Name:</b> {scenarioName}
              </span>
              <select
                className="select-dropdown control-name-input control-input"
                name="scenario_template_selector"
                value={template ?? ""}
                onChange={handleTemplateSelectChange}>
                <option key="default" value="">
                  &lt;no scenario&gt;
                </option>
                {sortedTemplates.map((scenario: [string, MetaData]) => (
                  <option key={scenario[1].name} value={scenario[0]}>
                    {scenario[1].name}
                  </option>
                ))}
              </select>
              <button className="control-input control-button blue-button" type="submit">
                Create
              </button>
            </form>
            <>
              <br />
              <br />
              <button className="blue-button tutorial-button" onClick={launchTutorial}>
                Tutorial
              </button>
            </>
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
