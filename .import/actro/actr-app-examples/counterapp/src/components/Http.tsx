import { useHttp } from 'askit';
import { useEffect } from 'react';

export const HttpRequest = () => {
  const { get } = useHttp();

  useEffect(() => {
    get<
      {
        code: number;
        message: string;
      },
      object
    >('unique', 'https://www.baidu.com').then((response) => {
      console.error('123', response);
    });
  }, []);
};
