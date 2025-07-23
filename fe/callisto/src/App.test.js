import * as React from 'react';
import { render } from '@testing-library/react';
//import App from './App';
import { Ship, EntityList, Planet, Missile } from 'lib/universal';
import '@testing-library/jest-dom';
import '@testing-library/jest-dom/extend-expect';

const ShipSerializeTest = () => {
  const ship = new Ship(
    "Test Ship",
    [0, 0, 0],
    [0, 30000, -30000],
    [[[0, 0, 0], 0], null],
    "Buccaneer",
    100,
    10,
    4,
    2,
    4,
    40,
    1,
    "Improved",
    [true],
    0,
    false,
    false,
    []
  );
  
  console.log("Serialize ship to JSON");
  const json = JSON.stringify(ship);
  console.log("Deserialize ship from JSON");
  const parsed = Ship.parse(JSON.parse(json));
  console.log("Compare ship to parsed ship");
  expect(ship).toEqual(parsed);
};

const EntitiesSerializeTest = () => {
  const entities = new EntityList();
  
  // Create and add a ship directly
  const ship = new Ship(
    "Test Ship",
    [0, 0, 0],
    [0, 30000, -30000],
    [[[0, 0, 0], 0], null],
    "Buccaneer",
    100,
    10,
    4,
    2,
    4,
    40,
    1,
    "Improved",
    [true],
    0,
    false,
    false,
    []
  );
  entities.ships.push(ship);
  
  // Add planets
  const planet1 = new Planet(
    "Test Planet",
    [1500, 2500, 3500],
    [10, 20, 30],
    "blue",
    null,
    6371000,
    5.97e24
  );
  entities.planets.push(planet1);
  
  const planet2 = new Planet(
    "Test Planet 2",
    [1000000, 500000, 750000],
    [15, 25, 35],
    "red",
    "Test Planet",
    3389000,
    6.39e23
  );
  entities.planets.push(planet2);
  
  // Add a missile
  const missile = new Missile(
    "Test Ship::Test Planet 2::0",
    [250000, 150000, 75000],
    [5000, 3000, 1500],
    [100, 200, 300]
  );
  entities.missiles.push(missile);
  
  console.log("Serialize entities to JSON");
  const json = JSON.stringify(entities);
  console.log("Deserialize entities from JSON");
  const parsed = EntityList.parse(JSON.parse(json));
  console.log("Compare entities to parsed entities");
  expect(entities).toEqual(parsed);
};

test("Test Ship Serialize",ShipSerializeTest);
test("Test Entities Serialize", EntitiesSerializeTest);
