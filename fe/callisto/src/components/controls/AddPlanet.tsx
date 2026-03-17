import * as React from "react";
import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { POSITION_SCALE } from "lib/universal";
import { Accordion } from "lib/Accordion";
import { Tooltip } from "react-tooltip";
import { CiCircleQuestion } from "react-icons/ci";
import {
  Planet,
  defaultPlanet,
  findPlanet,
  PlanetVisualEffect,
} from "lib/entities";

import { addPlanet } from "lib/serverManager";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

type AddShipProps = unknown;

export const AddPlanet: React.FC<AddShipProps> = () => {
  const entities = useAppSelector(entitiesSelector);
  const planetNameRef = useRef<HTMLInputElement>(null);

  const planetNames = useMemo(
    () => entities.planets.map((planet: Planet) => planet.name),
    [entities.ships],
  );
  const initialTemplate = useMemo(
    () => ({
      name: "Planet",
      xpos: "0",
      ypos: "0",
      zpos: "0",
      color: "yellow",
      mass: 5.972e24,
      primary: null as string | null,
      visual_effects: [] as PlanetVisualEffect[],
    }),
    [entities],
  );

  const [addPlanetData, setAddPlanetData] = useState(initialTemplate);

  useEffect(() => {
    const current =
      entities.planets.find((planet) => planet.name === addPlanetData.name) ||
      null;
    if (current != null) {
      const template = {
        name: current.name,
        xpos: (current.position[0] / POSITION_SCALE).toString(),
        ypos: (current.position[1] / POSITION_SCALE).toString(),
        zpos: (current.position[2] / POSITION_SCALE).toString(),
        mass: current.mass,
        color: current.color,
        primary: current.primary,
        visual_effects: [],
      };
      setAddPlanetData(template);
    }
  }, [addPlanetData.name, entities.ships]);

  const handleChange = useMemo(
    () => (event: React.ChangeEvent<HTMLInputElement>) => {
      event.target.style.color = "black";
      if (event.target.name === "name") {
        if (planetNames.includes(event.target.value)) {
          event.target.style.color = "green";
          const planet = findPlanet(entities, event.target.value);
          if (planet != null) {
            setAddPlanetData({
              name: event.target.value,
              xpos: (planet.position[0] / POSITION_SCALE).toString(),
              ypos: (planet.position[1] / POSITION_SCALE).toString(),
              zpos: (planet.position[2] / POSITION_SCALE).toString(),
              mass: planet.mass,
              color: planet.color,
              primary: planet.primary,
              visual_effects: planet.visual_effects,
            });
          }
        }
      }
      setAddPlanetData({
        ...addPlanetData,
        [event.target.name]: event.target.value,
      });
    },
    [planetNames, entities, setAddPlanetData, addPlanetData],
  );

  const handleSubmit = useCallback(
    (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      const position: [number, number, number] = [
        Number(addPlanetData.xpos) * POSITION_SCALE,
        Number(addPlanetData.ypos) * POSITION_SCALE,
        Number(addPlanetData.zpos) * POSITION_SCALE,
      ];
      const planet =
        findPlanet(entities, addPlanetData.name) || defaultPlanet();

      const revision: Planet = {
        ...planet,
        name: addPlanetData.name,
        position,
        mass: addPlanetData.mass,
        velocity: [0, 0, 0],
        primary: addPlanetData.primary,
        color: addPlanetData.color,
        visual_effects: addPlanetData.visual_effects,
      };

      addPlanet(revision);
      setAddPlanetData(initialTemplate);
      planetNameRef.current!.style.color = "black";
    },
    [addPlanetData, entities, initialTemplate, planetNameRef],
  );

  const handleColorChange = useCallback(
    (color: string) => setAddPlanetData({ ...addPlanetData, color }),
    [addPlanetData, setAddPlanetData],
  );

  const handlePrimaryChange = useCallback(
    (primary: string) => {
      setAddPlanetData({
        ...addPlanetData,
        primary: primary === "" ? null : primary,
      });
    },
    [addPlanetData, setAddPlanetData],
  );

  const updateOrAddLabel = useMemo(
    () => (planetNames.includes(addPlanetData.name) ? "Update" : "Add"),
    [addPlanetData.name, planetNames],
  );

  return (
    <Accordion id="add-planet-header" title="Add Planet" initialOpen={false}>
      <form id="add-planet" className="control-form" onSubmit={handleSubmit}>
        <div id="add-planet-top-part">
          <label className="control-label">
            Name
            <input
              id="add-ship-name-input"
              className="control-name-input control-input"
              name="name"
              type="text"
              onChange={handleChange}
              value={addPlanetData.name}
              ref={planetNameRef}
            />
          </label>
          <label className="control-label">
            Position (km)
            <div className="coordinate-input">
              <input
                className="control-input"
                name="xpos"
                type="text"
                value={addPlanetData.xpos}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="ypos"
                type="text"
                value={addPlanetData.ypos}
                onChange={handleChange}
              />
              <input
                className="control-input"
                name="zpos"
                type="text"
                value={addPlanetData.zpos}
                onChange={handleChange}
              />
            </div>
          </label>
          <div className="coordinate-input">
            <label className="control-label">
              Mass (kg)
              <input
                className="mass-input control-input"
                name="mass"
                type="text"
                value={addPlanetData.mass}
                onChange={handleChange}
              />
            </label>
            <PlanetList
              planetName={addPlanetData.primary}
              setPlanetName={handlePrimaryChange}
              planetNames={planetNames}
              exclude={addPlanetData.name}
            />
          </div>
        </div>
        <ColorChooser
          color={addPlanetData.color}
          setColor={handleColorChange}
        />
        <PlanetEffectChooser
          effects={addPlanetData.visual_effects}
          setEffects={(effects) =>
            setAddPlanetData({ ...addPlanetData, visual_effects: effects })
          }
        />
        <input
          className="control-input control-button blue-button"
          type="submit"
          value={updateOrAddLabel}
        />
      </form>
    </Accordion>
  );
};

function PlanetList(args: {
  planetName: string | null;
  setPlanetName: (planet: string) => void;
  planetNames: string[];
  exclude: string;
}) {
  const selectRef = useRef<HTMLSelectElement>(null);
  useEffect(() => {
    if (selectRef.current != null) {
      selectRef.current.value = args.planetName || "";
    }
  }, [args.planetName]);

  const handlePlanetListSelectChange = useCallback(
    (event: React.ChangeEvent<HTMLSelectElement>) => {
      const value = event.target.value;
      args.setPlanetName(value);
    },
    [args],
  );

  return (
    <>
      <div className="control-launch-div">
        <label className="control-label">
          Primary
          <select
            className="select-dropdown control-name-input control-input"
            name="planet_list_choice"
            ref={selectRef}
            defaultValue={args.planetName || ""}
            onChange={handlePlanetListSelectChange}
          >
            {[
              <option key="null-planet-list" value="">
                {"" as String}
              </option>,
            ].concat(
              Object.values(args.planetNames)
                .sort((a, b) => a.localeCompare(b))
                .filter((planet) => planet !== args.exclude)
                .map((planet) => (
                  <option
                    key={planet + "-planet_list"}
                    value={planet}
                  >{`${planet}`}</option>
                )),
            )}
          </select>
        </label>
      </div>
    </>
  );
}

function ColorChooser(args: {
  color: string;
  setColor: (color: string) => void;
}) {
  const colorRef = useRef<HTMLInputElement>(null);

  const handleChange = () => {
    args.setColor((colorRef.current && colorRef.current.value) || args.color);
  };
  return (
    <label className="control-label">
      Color
      <input
        className="mass-input control-input"
        name="color"
        type="text"
        value={args.color}
        onChange={handleChange}
        ref={colorRef}
      />
    </label>
  );
}

function PlanetEffectChooser(args: {
  effects: PlanetVisualEffect[];
  setEffects: (effects: PlanetVisualEffect[]) => void;
}) {
  const phongRef = useRef<HTMLInputElement>(null);
  const noiseRef = useRef<HTMLInputElement>(null);
  const stripedRef = useRef<HTMLInputElement>(null);
  const atmoRingRef = useRef<HTMLInputElement>(null);
  const planetRingRef = useRef<HTMLInputElement>(null);
  const latitudeRef = useRef<HTMLInputElement>(null);
  const cloudsRef = useRef<HTMLInputElement>(null);

  const handleChange = () => {
    let newEffects: PlanetVisualEffect[] = [];

    if (phongRef.current && phongRef.current.checked) {
      newEffects = [...newEffects, PlanetVisualEffect.PHONG_LIGHTING];
    }
    if (noiseRef.current && noiseRef.current.checked) {
      newEffects = [...newEffects, PlanetVisualEffect.NOISE_TEXTURE];
    }

    if (stripedRef.current && stripedRef.current.checked) {
      newEffects = [...newEffects, PlanetVisualEffect.STRIPED_BANDS];
    }

    if (atmoRingRef.current && atmoRingRef.current.checked) {
      newEffects = [...newEffects, PlanetVisualEffect.ATMOSPHERE_RING];
    }

    if (planetRingRef.current && planetRingRef.current.checked) {
      newEffects = [...newEffects, PlanetVisualEffect.PLANETARY_RING];
    }

    if (latitudeRef.current && latitudeRef.current.checked) {
      newEffects = [...newEffects, PlanetVisualEffect.LATITUDE_COLOR];
    }

    if (cloudsRef.current && cloudsRef.current.checked) {
      newEffects = [...newEffects, PlanetVisualEffect.ANIMATED_CLOUDS];
    }

    args.setEffects(newEffects);
  };

  return (
    <>
      <div className="control-label">Effects</div>
      <table className="planet-effect-table">
        <tbody>
          <tr>
            <td>
              <label className="control-label">
                Phong Lighting
                <input
                  className="planet-effect-checkbox"
                  name="phong"
                  type="checkbox"
                  ref={phongRef}
                  checked={args.effects.includes(
                    PlanetVisualEffect.PHONG_LIGHTING,
                  )}
                  onChange={handleChange}
                />
              </label>
            </td>
            <td>
              <label className="control-label">
                Noise Texture
                <input
                  className="planet-effect-checkbox"
                  name="noise"
                  type="checkbox"
                  ref={noiseRef}
                  checked={args.effects.includes(
                    PlanetVisualEffect.NOISE_TEXTURE,
                  )}
                  onChange={handleChange}
                />
              </label>
            </td>
          </tr>
          <tr>
            <td>
              <label className="control-label">
                Striped Bands
                <input
                  className="planet-effect-checkbox"
                  name="striped"
                  type="checkbox"
                  ref={stripedRef}
                  checked={args.effects.includes(
                    PlanetVisualEffect.STRIPED_BANDS,
                  )}
                  onChange={handleChange}
                />
              </label>
            </td>
            <td>
              <label className="control-label">
                Atmosphere Ring
                <input
                  className="planet-effect-checkbox"
                  name="atmoRing"
                  type="checkbox"
                  ref={atmoRingRef}
                  checked={args.effects.includes(
                    PlanetVisualEffect.ATMOSPHERE_RING,
                  )}
                  onChange={handleChange}
                />
              </label>
            </td>
          </tr>
          <tr>
            <td>
              <label className="control-label">
                Planetary Ring
                <input
                  className="planet-effect-checkbox"
                  name="planetRing"
                  type="checkbox"
                  ref={planetRingRef}
                  checked={args.effects.includes(
                    PlanetVisualEffect.PLANETARY_RING,
                  )}
                  onChange={handleChange}
                />
              </label>
            </td>
            <td>
              <label className="control-label">
                Latitude Color
                <input
                  className="planet-effect-checkbox"
                  name="latitude"
                  type="checkbox"
                  ref={latitudeRef}
                  checked={args.effects.includes(
                    PlanetVisualEffect.LATITUDE_COLOR,
                  )}
                  onChange={handleChange}
                />
              </label>
            </td>
          </tr>
          <tr>
            <td>
              <label className="control-label">
                Animated Clouds
                <input
                  className="planet-effect-checkbox"
                  name="clouds"
                  type="checkbox"
                  ref={cloudsRef}
                  checked={args.effects.includes(
                    PlanetVisualEffect.ANIMATED_CLOUDS,
                  )}
                  onChange={handleChange}
                />
              </label>
            </td>
          </tr>
        </tbody>
      </table>
    </>
  );
}
