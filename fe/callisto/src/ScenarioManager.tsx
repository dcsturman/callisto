import React from "react";
import {joinScenario, createScenario } from "./ServerManager";
import {Logout} from "./Authentication";

const TUTORIAL_SCENARIO = "tutorial.json";
export const TUTORIAL_PREFIX = "$TUTORIAL-";

// This function will generate a three word name (with hyphens between the words) or random words
// to be used as the name for a new scenario.
function generateUniqueHyphenatedName() {
    // eslint-disable-next-line array-element-newline
    const words = [
        "ocean", "mountain", "river", "forest", "desert", "island", "valley", "canyon",
        "meadow", "glacier", "sunset", "sunrise", "twilight", "midnight", "shadow", "light",
        "storm", "breeze", "whisper", "silence", "echo", "journey", "voyage", "adventure",
        "discovery", "secret", "mystery", "wonder", "magic", "dream", "fantasy", "reality",
        "truth", "wisdom", "courage", "strength", "peace", "joy", "laughter", "tears",
        "heart", "soul", "spirit", "mind", "thought", "idea", "vision", "passion",
        "grace", "beauty", "charm", "elegance", "style", "fashion", "art", "music",
        "dance", "theater", "cinema", "story", "poem", "novel", "legend", "myth",
        "destiny", "fate", "chance", "luck", "hope", "faith", "charity", "kindness",
        "friendship", "love", "family", "home", "garden", "flower", "tree", "bird",
        "fish", "star", "planet", "galaxy", "universe", "time", "space", "matter",
        "energy", "motion", "change", "growth", "creation", "destruction", "balance",
        "harmony", "silence", "sound", "color", "shape", "texture", "taste", "smell",
        "touch", "feeling", "emotion", "desire", "belief", "knowledge", "learning"
        ];
    const selectedWords = [];
    const availableIndices = [...words.keys()];

    while (selectedWords.length < 3 && availableIndices.length > 0) {
        const randomIndex = Math.floor(Math.random() * availableIndices.length);
        const wordIndex = availableIndices.splice(randomIndex, 1)[0];
        selectedWords.push(words[wordIndex]);
    }

    return selectedWords.join('-');
}

type ScenarioManagerProps = {
    scenarios: string[];
    scenarioTemplates: string[];
    setTutorialMode: (tutorialMode: boolean) => void;
    setAuthenticated: (authenticated: boolean) => void;
    email: string | null;
    setEmail: (email: string | null) => void;
};

export const ScenarioManager: React.FC<ScenarioManagerProps> = ({scenarios, scenarioTemplates, setTutorialMode, setAuthenticated, email, setEmail}) => {
    const sortedFilteredScenarios = scenarios.filter(scenario => !scenario.startsWith(TUTORIAL_PREFIX)).sort((a, b) => a.localeCompare(b));
    const sortedTemplates = scenarioTemplates.sort((a, b) => a.localeCompare(b));

    const [scenario, setScenario] = React.useState<string | null>(sortedFilteredScenarios[0]?? null);
    const [template, setTemplate] = React.useState<string | null>(null);

    const scenarioName = generateUniqueHyphenatedName();

    function handleTemplateSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
        setTemplate(event.target.value);
    }

    function handleScenarioSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
        setScenario(event.target.value);
    }

    function handleJoinScenario(event: React.FormEvent<HTMLFormElement>) {
        event.preventDefault();

        joinScenario(scenario?? "");
    }

    function handleCreateScenario(event: React.FormEvent<HTMLFormElement>) {
        event.preventDefault();
        createScenario(scenarioName, template?? "");
    }

    function launchTutorial() {
        setTutorialMode(true);
        
        const random_tutorial_name = TUTORIAL_PREFIX + generateUniqueHyphenatedName();
        createScenario(random_tutorial_name, TUTORIAL_SCENARIO);
    }   

    // TODO: Remove TUTORIAL scenarios.
    // TODO: It doesn't seem we're using the cloud scenarios, but something in the dockerfile instead.
    return (
        <>
        <div className="authentication-container">
            <div className="authentication-blurb">
                <h1 className="authentication-title">Scenarios</h1>
                <br />
                <br />
                From this screen you can create a scenario, join an existing one to play with others, or just try the tutorial.  
                Right now we show all scenarios to make Callisto easier to use.
                Out of courtesy, please do not join a scenario to which you haven&apos;t been invited!  
                <br />
                <br />
                Note that if you customize a scenario from a template, please note that after 5 minutes without any users logged in, the scenario will be deleted.
            </div>
            <br />
            <br />
            <form className="scenario-join-form" onSubmit={handleJoinScenario}>
                <h1>Join Existing Scenario</h1>
                <br />
                <select className="select-dropdown control-name-input control-input" name="scenario_selector" value={scenario?? ""} onChange={handleScenarioSelectChange}>
                    {sortedFilteredScenarios
                        .map((scenario) => (
                        <option key={scenario} value={scenario}>
                            {scenario}
                        </option>
                    ))}
                </select>
                <button className="control-input control-button blue-button" type="submit">Join</button>
            </form>
            <br />
            <br />
            <form className = "scenario-create-form" onSubmit={handleCreateScenario}>
                <h1>Create New Scenario</h1>
                <br />
                <span className = "label-scenario-name"><b>Name:</b> {scenarioName}</span>
                    <select className="select-dropdown control-name-input control-input" name="scenario_template_selector" value={template?? ""} onChange={handleTemplateSelectChange}>
                        <option key="default" value="">&lt;no scenario&gt;</option>
                        {sortedTemplates
                          .map((scenario) => (
                            <option key={scenario} value={scenario}>
                                {scenario}
                            </option>
                        ))}
                    </select>
                    <button className="control-input control-button blue-button" type="submit">Create</button>
            </form>
            <>
                <br />
                <br />
                <button
                    className="blue-button tutorial-button"
                    onClick={launchTutorial}>
                    Tutorial
                </button>
            </>
        </div>
        <div className="admin-button-window">
            <Logout setAuthenticated={setAuthenticated} email={email} setEmail={setEmail}/>
        </div>
        </>
      );
};
