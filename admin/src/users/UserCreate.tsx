import {
  Alert,
  Button,
  FieldError,
  Input,
  Label,
  TextField,
} from '@heroui/react';
import { TriangleAlert, UserPlus } from 'lucide-react';
import { CreateBase, Form } from 'ra-core';
import { useState } from 'react';
import { Controller, useFormContext, useFormState } from 'react-hook-form';

import type { UserRecord } from '../api/types';
import { PageHeader } from '../ui/PageHeader';

interface UserCreateInput {
  Name: string;
  Password: string;
}

export function UserCreate() {
  const [submitError, setSubmitError] = useState<unknown>();

  return (
    <CreateBase<UserCreateInput, UserRecord>
      disableAuthentication
      mutationMode="pessimistic"
      mutationOptions={{ onError: setSubmitError }}
      redirect="show"
      resource="users"
    >
      <UserCreateForm
        onSubmitStart={() => { setSubmitError(undefined); }}
        submitError={submitError}
      />
    </CreateBase>
  );
}

function UserCreateForm({
  onSubmitStart,
  submitError,
}: {
  onSubmitStart: () => void;
  submitError: unknown;
}) {
  return (
    <div className="space-y-6">
      <PageHeader
        breadcrumbs={[
          { label: 'Users', to: '/admin/users' },
          { label: 'Create user' },
        ]}
        description="Create a local account with an initial sign-in credential."
        title="Create user"
      />

      <Form<UserCreateInput>
        className="max-w-xl space-y-5"
        defaultValues={{ Name: '', Password: '' }}
        noValidate
      >
        <CreateFormContents onSubmitStart={onSubmitStart} submitError={submitError} />
      </Form>
    </div>
  );
}

function CreateFormContents({
  onSubmitStart,
  submitError,
}: {
  onSubmitStart: () => void;
  submitError: unknown;
}) {
  const { isSubmitting } = useFormState<UserCreateInput>();
  return (
    <>
      <CreateFields isPending={isSubmitting} />
      {submitError !== undefined && <CreateError error={submitError} />}
      <Button
        aria-busy={isSubmitting}
        isDisabled={isSubmitting}
        onPress={onSubmitStart}
        type="submit"
      >
        <UserPlus aria-hidden="true" className="size-4" />
        <span className="inline-flex min-h-5 items-center">Create user</span>
      </Button>
    </>
  );
}

function CreateFields({ isPending }: { isPending: boolean }) {
  const { control } = useFormContext<UserCreateInput>();

  return (
    <>
      <Controller
        control={control}
        name="Name"
        rules={{
          required: 'Name is required.',
          validate: (value) => value.trim().length > 0 || 'Name is required.',
        }}
        render={({ field, fieldState }) => (
          <TextField
            fullWidth
            isInvalid={fieldState.invalid}
            isRequired
            name={field.name}
          >
            <Label>Name</Label>
            <Input
              autoComplete="off"
              disabled={isPending}
              onBlur={field.onBlur}
              onChange={field.onChange}
              ref={field.ref}
              value={field.value}
            />
            <FieldError>{fieldState.error?.message}</FieldError>
          </TextField>
        )}
      />

      <Controller
        control={control}
        name="Password"
        rules={{ required: 'Password is required.' }}
        render={({ field, fieldState }) => (
          <TextField
            fullWidth
            isInvalid={fieldState.invalid}
            isRequired
            name={field.name}
          >
            <Label>Initial password</Label>
            <Input
              autoComplete="new-password"
              disabled={isPending}
              onBlur={field.onBlur}
              onChange={field.onChange}
              ref={field.ref}
              type="password"
              value={field.value}
            />
            <FieldError>{fieldState.error?.message}</FieldError>
          </TextField>
        )}
      />
    </>
  );
}

function CreateError({ error }: { error: unknown }) {
  const status = statusOf(error);
  const message = status === 400
    ? 'Check the submitted values and try again.'
    : status === 409
      ? 'A user with these details already exists.'
      : 'The server could not create this user.';

  return (
    <Alert role="alert" status="danger">
      <Alert.Indicator>
        <TriangleAlert aria-hidden="true" className="size-4" />
      </Alert.Indicator>
      <Alert.Content>
        <Alert.Title>User creation failed</Alert.Title>
        <Alert.Description>{message}</Alert.Description>
      </Alert.Content>
    </Alert>
  );
}

function statusOf(error: unknown): number | undefined {
  if (typeof error !== 'object' || error === null || !('status' in error)) return undefined;
  return typeof error.status === 'number' ? error.status : undefined;
}
