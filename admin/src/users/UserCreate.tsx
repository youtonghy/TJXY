import { Create, PasswordInput, SimpleForm, TextInput, required } from 'react-admin';

export function UserCreate() {
  return (
    <Create title="Create user" redirect="show" mutationMode="pessimistic">
      <SimpleForm sx={{ maxWidth: 560 }}>
        <TextInput source="Name" label="Name" autoComplete="off" fullWidth validate={required()} />
        <PasswordInput source="Password" label="Initial password" autoComplete="new-password" fullWidth validate={required()} />
      </SimpleForm>
    </Create>
  );
}
