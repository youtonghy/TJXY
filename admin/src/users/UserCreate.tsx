import {
  Alert,
  Button,
  FieldError,
  Input,
  Label,
  TextField,
} from '@heroui/react';
import { TriangleAlert, UserPlus } from 'lucide-react';
import { CreateBase, Form, useNotify, useRedirect } from 'ra-core';
import { useState } from 'react';
import { Controller, useFormContext, useFormState } from 'react-hook-form';

import type { UserRecord } from '../api/types';
import { useTranslate } from '../settings/i18n';
import { PageHeader } from '../ui/PageHeader';

interface UserCreateInput {
  Name: string;
  Password: string;
}

export function UserCreate() {
  const tr = useTranslate();
  const notify = useNotify();
  const redirect = useRedirect();
  const [submitError, setSubmitError] = useState<unknown>();

  return (
    <CreateBase<UserCreateInput, UserRecord>
      disableAuthentication
      mutationMode="pessimistic"
      mutationOptions={{
        onError: setSubmitError,
        onSuccess: (record) => {
          notify(tr('User created.', '用户已创建。'), { type: 'success' });
          redirect('show', 'users', record.id);
        },
      }}
      redirect={false}
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
  const tr = useTranslate();
  return (
    <div className="space-y-6">
      <PageHeader
        breadcrumbs={[
          { label: tr('Users', '用户'), to: '/admin/users' },
          { label: tr('Create user', '创建用户') },
        ]}
        description={tr('Create a local account with an initial sign-in credential.', '创建本地账户并设置初始登录密码。')}
        title={tr('Create user', '创建用户')}
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
  const tr = useTranslate();
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
        <span className="inline-flex min-h-5 items-center">{tr('Create user', '创建用户')}</span>
      </Button>
    </>
  );
}

function CreateFields({ isPending }: { isPending: boolean }) {
  const tr = useTranslate();
  const { control } = useFormContext<UserCreateInput>();

  return (
    <>
      <Controller
        control={control}
        name="Name"
        rules={{
          required: tr('Name is required.', '请输入名称。'),
          validate: (value) => value.trim().length > 0 || tr('Name is required.', '请输入名称。'),
        }}
        render={({ field, fieldState }) => (
          <TextField
            fullWidth
            isInvalid={fieldState.invalid}
            isRequired
            name={field.name}
          >
            <Label>{tr('Name', '名称')}</Label>
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
        rules={{ required: tr('Password is required.', '请输入密码。') }}
        render={({ field, fieldState }) => (
          <TextField
            fullWidth
            isInvalid={fieldState.invalid}
            isRequired
            name={field.name}
          >
            <Label>{tr('Initial password', '初始密码')}</Label>
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
  const tr = useTranslate();
  const status = statusOf(error);
  const message = status === 400
    ? tr('Check the submitted values and try again.', '请检查提交内容后重试。')
    : status === 409
      ? tr('A user with these details already exists.', '具有这些信息的用户已存在。')
      : tr('The server could not create this user.', '服务器无法创建该用户。');

  return (
    <Alert role="alert" status="danger">
      <Alert.Indicator>
        <TriangleAlert aria-hidden="true" className="size-4" />
      </Alert.Indicator>
      <Alert.Content>
        <Alert.Title>{tr('User creation failed', '用户创建失败')}</Alert.Title>
        <Alert.Description>{message}</Alert.Description>
      </Alert.Content>
    </Alert>
  );
}

function statusOf(error: unknown): number | undefined {
  if (typeof error !== 'object' || error === null || !('status' in error)) return undefined;
  return typeof error.status === 'number' ? error.status : undefined;
}
