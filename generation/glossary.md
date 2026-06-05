
##### Fabricate
Sometimes it's impossible to generate a valid value from a model, for example when there are no
entries and we want to generate an attribute value. Rather than erroring or panicing, generators
will *fabricate* a value - generate a value which is invalid by, for example, making up non-existant
ID's. This is often the case when the model is *anemic*.

##### Anemic model
A model which contains few entities, usually early on in a simulation or test case.